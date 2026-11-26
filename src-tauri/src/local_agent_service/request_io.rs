// Allows a 1 MiB clipboard even when JSON escapes every byte, plus protocol metadata.
const MAX_AGENT_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

struct AgentDeadlineStream {
    socket: TcpStream,
    deadline: Instant,
}

fn agent_io_remaining(deadline: Instant) -> std::io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "agent_request_deadline_exceeded",
            )
        })
}

impl Read for AgentDeadlineStream {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.socket
            .set_read_timeout(Some(agent_io_remaining(self.deadline)?))?;
        self.socket.read(bytes)
    }
}

impl Write for AgentDeadlineStream {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.socket
            .set_write_timeout(Some(agent_io_remaining(self.deadline)?))?;
        self.socket.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        agent_io_remaining(self.deadline)?;
        self.socket.flush()
    }
}

/// Limit before buffering, and require the newline that terminates the JSON frame.
fn read_bounded_agent_response(reader: impl Read) -> Result<AgentResponse, String> {
    let mut reader = BufReader::new(reader.take(MAX_AGENT_RESPONSE_BYTES as u64 + 1));
    let mut line = Vec::new();
    let bytes = reader
        .read_until(b'\n', &mut line)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
                format!("agent_response_timeout: {error}")
            }
            _ => format!("agent_response_read_failed: {error}"),
        })?;
    if bytes > MAX_AGENT_RESPONSE_BYTES {
        return Err("agent_response_too_large".into());
    }
    if bytes == 0 {
        return Err("agent_response_empty".into());
    }
    if line.last() != Some(&b'\n') {
        return Err("agent_response_incomplete".into());
    }
    serde_json::from_slice(&line).map_err(|_| "agent_response_decode_failed".into())
}
