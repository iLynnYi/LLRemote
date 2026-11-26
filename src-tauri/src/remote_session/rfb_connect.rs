use std::future::Future;
use std::io;
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const CONNECT_POLL: Duration = Duration::from_millis(50);

enum ConnectError {
    Cancelled,
    Failed,
    TimedOut,
}

/// Runs only on the existing RFB worker. Numeric addresses avoid Tokio DNS / its
/// blocking pool; dropping the pending future closes its owned connecting socket.
pub(super) fn connect_addresses(
    addresses: impl IntoIterator<Item = SocketAddr>,
    stop: &AtomicBool,
    timeout: Duration,
) -> Result<TcpStream, String> {
    if stop.load(Ordering::Acquire) {
        return Err("rfb_cancelled".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|_| "rfb_tcp_connect_failed")?;
    runtime.block_on(async {
        let deadline = Instant::now() + timeout;
        for address in addresses {
            match await_connect(tokio::net::TcpStream::connect(address), stop, deadline).await {
                Ok(stream) => {
                    let stream = stream.into_std().map_err(|_| "rfb_socket_options_failed")?;
                    // The protocol transport uses blocking reads with short timeouts.
                    stream
                        .set_nonblocking(false)
                        .map_err(|_| "rfb_socket_options_failed")?;
                    return Ok(stream);
                }
                Err(ConnectError::Cancelled) => return Err("rfb_cancelled".into()),
                Err(ConnectError::TimedOut) => break,
                Err(ConnectError::Failed) => {}
            }
        }
        Err("rfb_tcp_connect_failed".into())
    })
}

async fn await_connect<T>(
    operation: impl Future<Output = io::Result<T>>,
    stop: &AtomicBool,
    deadline: Instant,
) -> Result<T, ConnectError> {
    let mut operation = Box::pin(operation);
    loop {
        if stop.load(Ordering::Acquire) {
            return Err(ConnectError::Cancelled);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(ConnectError::TimedOut);
        }
        // Only the borrowed wait expires; keep the same connect future and
        // socket, rather than restarting a SYN attempt on every poll.
        if let Ok(result) =
            tokio::time::timeout(remaining.min(CONNECT_POLL), operation.as_mut()).await
        {
            if stop.load(Ordering::Acquire) {
                return Err(ConnectError::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(ConnectError::TimedOut);
            }
            return result.map_err(|_| ConnectError::Failed);
        }
    }
}

#[cfg(test)]
#[path = "../../tests/support/rfb_connect_tests.rs"]
mod tests;
