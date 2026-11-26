const RFB_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const RFB_IO_POLL: Duration = Duration::from_millis(250);
const RFB_MESSAGE_WRITE_TIMEOUT: Duration = Duration::from_secs(15);

// Callers share a deadline across all fields; partial I/O cannot renew it.
struct RfbDeadlineIo<'a> {
    stream: &'a mut TcpStream,
    stop_requested: &'a AtomicBool,
    deadline: std::time::Instant,
}

impl<'a> RfbDeadlineIo<'a> {
    fn for_message(stream: &'a mut TcpStream, stop_requested: &'a AtomicBool) -> Self {
        Self {
            stream,
            stop_requested,
            deadline: std::time::Instant::now() + RFB_MESSAGE_WRITE_TIMEOUT,
        }
    }

    fn remaining(&self) -> Result<Duration, std::io::Error> {
        if self.stop_requested.load(Ordering::SeqCst) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "local VNC server stop requested",
            ));
        }
        self.deadline
            .checked_duration_since(std::time::Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "RFB I/O deadline exceeded")
            })
    }

    fn read_exact(&mut self, mut buffer: &mut [u8]) -> Result<(), std::io::Error> {
        while !buffer.is_empty() {
            let timeout = self.remaining()?.min(RFB_IO_POLL);
            self.stream.set_read_timeout(Some(timeout))?;
            match self.stream.read(buffer) {
                Ok(0) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "connection closed",
                    ));
                }
                Ok(count) => buffer = &mut buffer[count..],
                Err(error) if Self::retryable(&error) => continue,
                Err(error) => return Err(error),
            }
        }
        self.remaining().map(|_| ())
    }

    fn write_all(&mut self, buffer: &[u8]) -> Result<(), std::io::Error> {
        Write::write_all(self, buffer)?;
        self.remaining_write().map(|_| ())
    }

    fn remaining_write(&self) -> Result<Duration, std::io::Error> {
        // Buffered writers retry Interrupted. Cancellation must be terminal even
        // when a partial message is still buffered and the peer is not reading.
        self.remaining().map_err(|error| {
            if error.kind() == std::io::ErrorKind::Interrupted {
                std::io::Error::new(std::io::ErrorKind::ConnectionAborted, error)
            } else {
                error
            }
        })
    }

    fn retryable(error: &std::io::Error) -> bool {
        matches!(
            error.kind(),
            std::io::ErrorKind::TimedOut
                | std::io::ErrorKind::WouldBlock
                | std::io::ErrorKind::Interrupted
        )
    }
}

// Serializers share the caller's absolute deadline, including buffered flush and
// multi-part control responses. Neither successful fragments nor retries renew it.
impl Write for RfbDeadlineIo<'_> {
    fn write(&mut self, buffer: &[u8]) -> Result<usize, std::io::Error> {
        if buffer.is_empty() {
            return Ok(0);
        }
        loop {
            let timeout = self.remaining_write()?.min(RFB_IO_POLL);
            self.stream.set_write_timeout(Some(timeout))?;
            match self.stream.write(buffer) {
                Ok(0) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "connection stopped accepting data",
                    ));
                }
                Ok(count) => {
                    self.remaining_write()?;
                    return Ok(count);
                }
                Err(error) if Self::retryable(&error) => continue,
                Err(error) => return Err(error),
            }
        }
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        self.remaining_write()?;
        self.stream.flush()?;
        self.remaining_write().map(|_| ())
    }
}
