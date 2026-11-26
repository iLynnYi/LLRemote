fn serve_rfb_client(
    stream: &mut TcpStream,
    stop_requested: &AtomicBool,
    context: &RfbServerContext,
    client_id: String,
) -> Result<(), std::io::Error> {
    let shared_display = &context.shared_display;
    let framebuffer_core = &context.framebuffer_core;
    let mut handshake = RfbDeadlineIo {
        stream,
        stop_requested,
        deadline: std::time::Instant::now() + RFB_HANDSHAKE_TIMEOUT,
    };
    if !authenticate_rfb_client(&mut handshake, &context.auth_key)? {
        return Ok(());
    }

    let mut client_init = [0_u8; 1];
    handshake.read_exact(&mut client_init)?;
    let initial_display = shared_display
        .read()
        .map_err(|_| std::io::Error::other("display state poisoned"))?
        .clone();
    let mut server_init = Vec::new();
    write_server_init(&mut server_init, initial_display.framebuffer)?;
    handshake.write_all(&server_init)?;
    framebuffer_core
        .attach_client(client_id.clone())
        .map_err(std::io::Error::other)?;
    let _attachment = ServerClientAttachment {
        core: framebuffer_core,
        client_id: &client_id,
    };
    run_client_message_loop(
        stream,
        stop_requested,
        context,
        client_id.clone(),
        initial_display,
    )
}

// Authentication completes before admitting the connection to framebuffer delivery.
fn authenticate_rfb_client(
    stream: &mut RfbDeadlineIo<'_>,
    auth_key: &[u8; 8],
) -> Result<bool, std::io::Error> {
    stream.write_all(RFB_VERSION)?;

    let mut client_version = [0_u8; 12];
    stream.read_exact(&mut client_version)?;
    if &client_version[..8] != b"RFB 003."
        || !client_version[8..11].iter().all(u8::is_ascii_digit)
        || client_version[11] != b'\n'
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid RFB client version",
        ));
    }
    let minor = &client_version[8..11];
    // RFC 6143: unknown 3.x versions use the 3.3 handshake.
    if minor != b"007" && minor != b"008" {
        stream.write_all(&2_u32.to_be_bytes())?;
    } else {
        stream.write_all(&[1, 2])?;
        let mut selected_security = [0_u8; 1];
        stream.read_exact(&mut selected_security)?;
        if selected_security[0] != 2 {
            return Ok(false);
        }
    }

    let challenge = create_auth_challenge();
    stream.write_all(&challenge)?;

    let mut response = [0_u8; 16];
    stream.read_exact(&mut response)?;
    let expected = create_vnc_auth_response_from_key(auth_key, &challenge);
    if response != expected {
        stream.write_all(&1_u32.to_be_bytes())?;
        if minor == b"008" {
            let reason = b"Authentication failed";
            stream.write_all(&(reason.len() as u32).to_be_bytes())?;
            stream.write_all(reason)?;
        }
        return Ok(false);
    }

    stream.write_all(&0_u32.to_be_bytes())?;

    Ok(true)
}
