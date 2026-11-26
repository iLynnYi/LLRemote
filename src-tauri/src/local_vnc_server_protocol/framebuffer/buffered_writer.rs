// Explicit flush is the only delivery boundary. Drop discards residual bytes on
// failure or unwind instead of allowing BufWriter to retry a partial RFB message.
struct BufferedFramebufferWrite<W: Write> {
    inner: Option<BufWriter<W>>,
}

impl<W: Write> BufferedFramebufferWrite<W> {
    fn new(stream: W) -> Self {
        Self {
            inner: Some(BufWriter::with_capacity(
                FRAMEBUFFER_UPDATE_WRITE_BUFFER_SIZE,
                stream,
            )),
        }
    }
}

impl<W: Write> Write for BufferedFramebufferWrite<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.inner.as_mut().expect("live writer").write(bytes)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.as_mut().expect("live writer").flush()
    }
}

impl<W: Write> Drop for BufferedFramebufferWrite<W> {
    fn drop(&mut self) {
        if let Some(writer) = self.inner.take() {
            let _ = writer.into_parts();
        }
    }
}
