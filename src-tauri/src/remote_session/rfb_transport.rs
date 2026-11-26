use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const MESSAGE_WRITE_TIMEOUT: Duration = Duration::from_secs(2);
const WRITE_POLL: Duration = Duration::from_millis(250);

/// Native socket policy; protocol serializers keep their Read + Write seam.
pub(super) struct RfbTcpTransport<'a> {
    stream: &'a mut TcpStream,
    stop: &'a AtomicBool,
}

impl<'a> RfbTcpTransport<'a> {
    pub(super) fn new(stream: &'a mut TcpStream, stop: &'a AtomicBool) -> Self {
        Self { stream, stop }
    }
}

impl Read for RfbTcpTransport<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.stream.read(bytes)
    }
}

impl Write for RfbTcpTransport<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        check_stop(self.stop)?;
        self.stream.set_write_timeout(Some(WRITE_POLL))?;
        self.stream.write(bytes)
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        write_before_deadline(
            bytes,
            self.stop,
            Instant::now() + MESSAGE_WRITE_TIMEOUT,
            Instant::now,
            |remaining_bytes, timeout| {
                self.stream.set_write_timeout(Some(timeout))?;
                self.stream.write(remaining_bytes)
            },
        )
    }

    fn flush(&mut self) -> io::Result<()> {
        check_stop(self.stop)?;
        // TcpStream has no user-space output buffer; it cannot resend a suffix on drop.
        self.stream.flush()
    }
}

fn check_stop(stop: &AtomicBool) -> io::Result<()> {
    if stop.load(Ordering::Acquire) {
        Err(io::ErrorKind::ConnectionAborted.into())
    } else {
        Ok(())
    }
}

// One deadline covers successful fragments and transient I/O retries. The native
// closure also shortens the system-call timeout to the remaining budget.
fn write_before_deadline(
    mut bytes: &[u8],
    stop: &AtomicBool,
    deadline: Instant,
    mut now: impl FnMut() -> Instant,
    mut write: impl FnMut(&[u8], Duration) -> io::Result<usize>,
) -> io::Result<()> {
    loop {
        check_stop(stop)?;
        let remaining = deadline
            .checked_duration_since(now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(io::ErrorKind::TimedOut)?;
        if bytes.is_empty() {
            return Ok(());
        }
        match write(bytes, remaining.min(WRITE_POLL)) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(count) => bytes = &bytes[count..],
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut
                        | io::ErrorKind::WouldBlock
                        | io::ErrorKind::Interrupted
                ) =>
            {
                continue
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/support/rfb_client_write_deadline_tests.rs"]
mod tests;
