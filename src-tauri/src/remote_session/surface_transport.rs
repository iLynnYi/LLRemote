use crate::remote_desktop_core::{ClientMemoryBudget, MemoryReservation, SurfaceFramePacket};
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tungstenite::handshake::server::Callback;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{accept_hdr_with_config, Error as WebSocketError, Message, WebSocket};

const MAX_CONNECTIONS: usize = 8;
const CONNECTION_ALLOWANCE: usize = 2 * 1024 * 1024;
const HANDSHAKE_BYTES: usize = 64 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(2);
pub(super) const ACK_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) struct SurfaceAdmission {
    active: Arc<AtomicUsize>,
    memory: ClientMemoryBudget,
}

pub(super) struct SurfaceConnectionPermit {
    active: Arc<AtomicUsize>,
    _memory: MemoryReservation,
}

impl SurfaceAdmission {
    pub(super) fn new(memory: ClientMemoryBudget) -> Self {
        Self {
            active: Arc::new(AtomicUsize::new(0)),
            memory,
        }
    }

    pub(super) fn acquire(&self) -> Result<SurfaceConnectionPermit, String> {
        self.active
            .try_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < MAX_CONNECTIONS).then_some(active + 1)
            })
            .map_err(|_| "surface_connection_limit")?;
        match self.memory.reserve(CONNECTION_ALLOWANCE) {
            Ok(memory) => Ok(SurfaceConnectionPermit {
                active: Arc::clone(&self.active),
                _memory: memory,
            }),
            Err(error) => {
                self.active.fetch_sub(1, Ordering::AcqRel);
                Err(error)
            }
        }
    }
}

impl Drop for SurfaceConnectionPermit {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Absolute deadlines and a handshake byte cap apply to every underlying read /
/// write, including fragmented WebSocket messages and slow HTTP header trickles.
struct SurfaceSocket {
    stream: TcpStream,
    remaining_handshake: Option<usize>,
    read_deadline: Option<Instant>,
    write_deadline: Option<Instant>,
}

impl SurfaceSocket {
    fn remaining(deadline: Option<Instant>, maximum: Duration) -> io::Result<Duration> {
        let remaining = deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
            .unwrap_or(maximum)
            .min(maximum);
        if remaining.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        Ok(remaining)
    }
}

impl Read for SurfaceSocket {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        self.stream.set_read_timeout(Some(Self::remaining(
            self.read_deadline,
            Duration::from_millis(250),
        )?))?;
        let limit = self
            .remaining_handshake
            .unwrap_or(output.len())
            .min(output.len());
        if limit == 0 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let read = self.stream.read(&mut output[..limit])?;
        if let Some(remaining) = &mut self.remaining_handshake {
            *remaining -= read;
        }
        Ok(read)
    }
}

impl Write for SurfaceSocket {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        self.stream
            .set_write_timeout(Some(Self::remaining(self.write_deadline, IO_TIMEOUT)?))?;
        self.stream.write(input)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

/// The socket is destroyed before its leases. The writer charge covers retained
/// Vec capacity (including geometric growth); in-flight packet ownership keeps
/// its own charge until the Canvas ACK or connection teardown.
pub(super) struct SurfaceTransport {
    socket: WebSocket<SurfaceSocket>,
    writer: Option<MemoryReservation>,
    in_flight: Option<MemoryReservation>,
    writer_bytes: usize,
    memory: ClientMemoryBudget,
}

impl SurfaceTransport {
    pub(super) fn accept<C: Callback>(
        stream: TcpStream,
        callback: C,
        memory: ClientMemoryBudget,
    ) -> Result<Self, ()> {
        // Accepted sockets can inherit the listener's nonblocking mode on macOS.
        // This transport uses blocking I/O with explicit absolute deadlines.
        stream.set_nonblocking(false).map_err(|_| ())?;
        let deadline = Instant::now() + IO_TIMEOUT;
        let socket = SurfaceSocket {
            stream,
            remaining_handshake: Some(HANDSHAKE_BYTES),
            read_deadline: Some(deadline),
            write_deadline: Some(deadline),
        };
        let config = WebSocketConfig::default()
            .read_buffer_size(4096)
            .write_buffer_size(0)
            .max_write_buffer_size(128 * 1024 * 1024 + 4096)
            .max_message_size(Some(256))
            .max_frame_size(Some(256));
        let mut socket = accept_hdr_with_config(socket, callback, Some(config)).map_err(|_| ())?;
        socket.get_mut().remaining_handshake = None;
        socket.get_mut().read_deadline = None;
        socket.get_mut().write_deadline = None;
        Ok(Self {
            socket,
            writer: None,
            in_flight: None,
            writer_bytes: 0,
            memory,
        })
    }

    pub(super) fn send_packet(&mut self, packet: SurfaceFramePacket) -> Result<(), ()> {
        let needed = packet
            .bytes()
            .len()
            .checked_add(16 * 1024)
            .and_then(|bytes| bytes.checked_mul(2))
            .ok_or(())?;
        let old_writer = if needed > self.writer_bytes {
            let next = self.memory.reserve(needed).map_err(|_| ())?;
            self.writer_bytes = needed;
            self.writer.replace(next)
        } else {
            None
        };
        let (bytes, reservation) = packet.into_transport_parts();
        self.in_flight = Some(reservation);
        self.socket.get_mut().write_deadline = Some(Instant::now() + IO_TIMEOUT);
        let result = self
            .socket
            .send(Message::Binary(bytes.into()))
            .map_err(|_| ());
        self.socket.get_mut().write_deadline = None;
        drop(old_writer);
        result
    }

    pub(super) fn begin_ack(&mut self, deadline: Instant) {
        self.socket.get_mut().read_deadline = Some(deadline);
        self.socket.get_mut().write_deadline = Some(deadline);
    }

    pub(super) fn read(&mut self) -> Result<Message, WebSocketError> {
        self.socket.read()
    }

    pub(super) fn presented(&mut self) {
        self.in_flight = None;
        self.socket.get_mut().read_deadline = None;
        self.socket.get_mut().write_deadline = None;
    }

    pub(super) fn poll_idle(&mut self) -> Result<(), ()> {
        let deadline = Instant::now() + Duration::from_millis(8);
        self.begin_ack(deadline);
        let result = match self.read() {
            Ok(Message::Close(_)) => Err(()),
            Ok(_) => Ok(()),
            Err(WebSocketError::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                Ok(())
            }
            Err(_) => Err(()),
        };
        self.socket.get_mut().read_deadline = None;
        self.socket.get_mut().write_deadline = None;
        result
    }
}

#[cfg(test)]
#[path = "../../tests/support/surface_transport_budget_tests.rs"]
mod tests;
