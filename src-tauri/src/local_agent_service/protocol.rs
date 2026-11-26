#[derive(Clone)]
struct AgentBootstrap {
    key: String,
    tickets: AgentTicketStore,
}

fn run_agent_listener(
    listener: TcpListener,
    stop_requested: Arc<AtomicBool>,
    bootstrap: AgentBootstrap,
    server_id: String,
    transfers: AgentTransferMap,
    inbox_dir: PathBuf,
    context: AgentRuntimeContext,
) {
    let sessions = Arc::new(Mutex::new(HashMap::<String, Instant>::new()));
    let mut clients = crate::local_vnc_server::LocalServerClientWorkers::new(
        Arc::clone(&stop_requested),
        "vnc-server-agent",
    );
    while !stop_requested.load(Ordering::SeqCst) {
        clients.reap_finished();
        match listener.accept() {
            Ok((stream, _)) => {
                if stop_requested.load(Ordering::SeqCst) {
                    break;
                }
                if !clients.has_capacity() || stream.set_nonblocking(false).is_err() {
                    continue;
                }
                let Ok(shutdown_socket) = stream.try_clone().map(Arc::new) else {
                    continue;
                };
                let bootstrap = bootstrap.clone();
                let server_id = server_id.clone();
                let sessions = Arc::clone(&sessions);
                let transfers = Arc::clone(&transfers);
                let inbox_dir = inbox_dir.clone();
                let context = context.clone();
                let _ = clients.spawn(stream, shutdown_socket, move |stream| {
                    handle_agent_stream(
                        stream, &bootstrap, &server_id, &sessions, &transfers, &inbox_dir, context,
                    );
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(_) => break,
        }
    }
}

fn handle_agent_stream(
    stream: TcpStream,
    bootstrap: &AgentBootstrap,
    server_id: &str,
    sessions: &Arc<Mutex<HashMap<String, Instant>>>,
    transfers: &AgentTransferMap,
    inbox_dir: &Path,
    context: AgentRuntimeContext,
) {
    let timeout = Some(Duration::from_secs(AGENT_REQUEST_TIMEOUT_SECONDS));
    if stream.set_read_timeout(timeout).is_err() || stream.set_write_timeout(timeout).is_err() {
        return;
    }
    let mut response = AgentResponse {
        ok: false,
        error: None,
        transfer_id: None,
        chunk_size: None,
        offset: None,
        file_name: None,
        remote_path: None,
        bytes_written: None,
        sha256: None,
        permissions: None,
        displays: None,
        secure_input: None,
        session_id: None,
        description: None,
        text: None,
        revision: None,
    };
    let result = read_agent_request(&stream).and_then(|request| {
        handle_agent_request(
            request, bootstrap, server_id, sessions, transfers, inbox_dir, context,
        )
    });

    match result {
        Ok(success) => response = success,
        Err(error) => response.error = Some(error),
    }

    let mut stream = stream;
    let _ = serde_json::to_writer(&mut stream, &response);
    let _ = stream.write_all(b"\n");
}

fn read_agent_request(stream: &TcpStream) -> Result<AgentRequest, String> {
    // Stop reading at the budget before allocating an arbitrarily long line.
    let mut reader = BufReader::new(stream).take(MAX_REQUEST_BYTES as u64 + 1);
    let mut line = String::new();
    let bytes = reader
        .read_line(&mut line)
        .map_err(|error| format!("agent_request_read_failed: {error}"))?;

    if bytes > MAX_REQUEST_BYTES {
        return Err("agent_request_too_large".to_string());
    }

    serde_json::from_str(&line).map_err(|error| format!("agent_request_decode_failed: {error}"))
}

fn handle_agent_request(
    request: AgentRequest,
    bootstrap: &AgentBootstrap,
    server_id: &str,
    sessions: &Arc<Mutex<HashMap<String, Instant>>>,
    transfers: &AgentTransferMap,
    inbox_dir: &Path,
    context: AgentRuntimeContext,
) -> Result<AgentResponse, String> {
    if request.message_type == "agent.session.open" {
        let supplied = request.bootstrap_key.as_deref().unwrap_or_default();
        if supplied != bootstrap.key && !consume_agent_ticket(&bootstrap.tickets, supplied) {
            return Err("agent_auth_failed".to_string());
        }
        let session_id = Uuid::new_v4().to_string();
        let mut session_guard = sessions
            .lock()
            .map_err(|_| "agent_session_state_poisoned".to_string())?;
        let now = Instant::now();
        session_guard.retain(|_, expires_at| *expires_at > now);
        session_guard.insert(session_id.clone(), now + AGENT_SESSION_TTL);
        let mut response = ok_response();
        response.session_id = Some(session_id);
        return Ok(response);
    }

    let session_id = request
        .session_id
        .as_deref()
        .ok_or_else(|| "agent_session_required".to_string())?;
    let is_valid = {
        let mut sessions = sessions
            .lock()
            .map_err(|_| "agent_session_state_poisoned".to_string())?;
        let now = Instant::now();
        sessions.retain(|_, expires_at| *expires_at > now);
        if let Some(expires_at) = sessions.get_mut(session_id) {
            *expires_at = now + AGENT_SESSION_TTL;
            true
        } else {
            false
        }
    };
    if !is_valid {
        return Err("agent_session_invalid".to_string());
    }

    if request.message_type == "agent.session.describe" {
        let mut response = ok_response();
        response.description = Some(LocalRuntimeAgentDescription {
            server_id: server_id.to_string(),
            view_only: context.view_only,
            capabilities: agent_capabilities_for_context(&context),
        });
        return Ok(response);
    }

    let view_only_allowed_action = matches!(
        request.message_type.as_str(),
        CLIPBOARD_TEXT_READ
            | AGENT_PERMISSIONS_DIAGNOSE
            | AGENT_DISPLAY_ENUMERATE
            | AGENT_DISPLAY_SELECT
            | SESSION_END
    );
    if context.view_only && !view_only_allowed_action {
        return Err("agent_view_only".into());
    }
    let needs_user_desktop = matches!(
        request.message_type.as_str(),
        CLIPBOARD_TEXT_READ
            | CLIPBOARD_TEXT_WRITE
            | MESSAGE_SHOW
            | FILE_OPEN
            | FILE_REVEAL
            | AGENT_DISPLAY_SELECT
            | DISPLAY_WAKE
            | REMOTE_DESKTOP_CONTROL
            | SESSION_END
    );
    if needs_user_desktop
        && context
            .interactive
            .as_ref()
            .is_some_and(|active| !active.load(Ordering::SeqCst))
    {
        return Err("agent_user_desktop_unavailable".into());
    }

    match request.message_type.as_str() {
        MESSAGE_SHOW => {
            crate::remote_message::show(request.text.as_deref().unwrap_or_default())?;
            Ok(ok_response())
        }
        CLIPBOARD_TEXT_WRITE => {
            let text = request.text.unwrap_or_default();
            if text.len() > MAX_REQUEST_BYTES {
                return Err("agent_clipboard_text_too_large".to_string());
            }
            set_clipboard_text_from_vnc(&text)?;
            Ok(ok_response())
        }
        CLIPBOARD_TEXT_READ => {
            let text = get_clipboard_text_for_vnc()?.unwrap_or_default();
            let mut response = ok_response();
            response.revision = Some(format!("{:x}", Sha256::digest(text.as_bytes())));
            response.text = Some(text);
            Ok(response)
        }
        FILE_TRANSFER_CREATE => create_file_transfer(request, transfers, inbox_dir),
        FILE_TRANSFER_RESUME => resume_file_transfer(request, transfers),
        FILE_TRANSFER_STATUS => status_file_transfer(request, transfers),
        FILE_TRANSFER_CHUNK => write_file_transfer_chunk(request, transfers),
        FILE_TRANSFER_COMMIT => commit_file_transfer(request, transfers),
        FILE_TRANSFER_PAUSE => pause_file_transfer(request, transfers),
        FILE_TRANSFER_CANCEL => cancel_file_transfer(request, transfers),
        FILE_REVEAL => reveal_file_response(request),
        FILE_OPEN => open_file_response(request),
        AGENT_PERMISSIONS_DIAGNOSE => diagnose_permissions_response(inbox_dir),
        AGENT_DISPLAY_ENUMERATE => enumerate_displays_response(),
        AGENT_DISPLAY_SELECT => select_display_response(request, context),
        DISPLAY_WAKE => wake_display_response(),
        SESSION_END => {
            crate::background_share::disconnect_lock::session_ended()?;
            // OS lock requests are asynchronous; acknowledgement is not lock-state proof.
            Ok(ok_response())
        }
        REMOTE_DESKTOP_CONTROL => remote_desktop_control_response(request),
        AGENT_SECURE_INPUT_RECOVER => recover_secure_input_response(request),
        _ => Err("agent_unsupported_request".to_string()),
    }
}

fn ok_response() -> AgentResponse {
    AgentResponse {
        ok: true,
        error: None,
        transfer_id: None,
        chunk_size: None,
        offset: None,
        file_name: None,
        remote_path: None,
        bytes_written: None,
        sha256: None,
        permissions: None,
        displays: None,
        secure_input: None,
        session_id: None,
        description: None,
        text: None,
        revision: None,
    }
}

fn send_agent_request(
    host: &str,
    port: u16,
    payload: serde_json::Value,
) -> Result<AgentResponse, String> {
    send_agent_request_with_timeout(
        host,
        port,
        payload,
        Duration::from_secs(AGENT_REQUEST_TIMEOUT_SECONDS),
    )
}

/// Sends one authenticated Agent protocol request to a remote Server Agent.
pub(crate) fn invoke_remote_agent(
    host: &str,
    port: u16,
    message: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let response = send_agent_request(host, port, message)?;
    serde_json::to_value(response).map_err(|error| format!("agent_response_encode_failed: {error}"))
}

fn send_agent_request_with_timeout(
    host: &str,
    port: u16,
    payload: serde_json::Value,
    timeout: Duration,
) -> Result<AgentResponse, String> {
    let deadline = Instant::now() + timeout;
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|error| format!("agent_address_resolve_failed: {error}"))?;
    let mut connected = None;
    for address in addresses {
        let remaining =
            agent_io_remaining(deadline).map_err(|_| "agent_request_timeout".to_string())?;
        if let Ok(stream) = TcpStream::connect_timeout(&address, remaining) {
            connected = Some(stream);
            break;
        }
    }
    let mut stream = AgentDeadlineStream {
        socket: connected.ok_or_else(|| "agent_unreachable".to_string())?,
        deadline,
    };
    serde_json::to_writer(&mut stream, &payload)
        .map_err(|error| format!("agent_request_encode_failed: {error}"))?;
    stream
        .write_all(b"\n")
        .map_err(|error| format!("agent_request_write_failed: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("agent_request_flush_failed: {error}"))?;

    read_bounded_agent_response(&mut stream)
}
