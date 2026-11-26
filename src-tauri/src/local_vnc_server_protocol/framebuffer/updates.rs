pub(super) fn write_desktop_size_update<W: Write>(
    stream: &mut W,
    framebuffer: DisplaySize,
) -> Result<(), std::io::Error> {
    stream.write_all(&[0, 0])?;
    stream.write_all(&1_u16.to_be_bytes())?;
    stream.write_all(&0_u16.to_be_bytes())?;
    stream.write_all(&0_u16.to_be_bytes())?;
    stream.write_all(&(framebuffer.width as u16).to_be_bytes())?;
    stream.write_all(&(framebuffer.height as u16).to_be_bytes())?;
    stream.write_all(&(-223_i32).to_be_bytes())?;
    Ok(())
}

pub(super) fn is_recoverable_framebuffer_source_error(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::InvalidData
        && error.to_string().contains("framebuffer_length_mismatch")
}
