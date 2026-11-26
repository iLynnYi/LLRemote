struct ServerConnectionClose {
    shutdown: Arc<TcpStream>,
    inbox: Arc<ClientInbox>,
}

// Core registration follows the connection owner, including writer unwind.
struct ServerClientAttachment<'a> {
    core: &'a crate::remote_desktop_core::ServerFramebufferCore,
    client_id: &'a str,
}

impl Drop for ServerClientAttachment<'_> {
    fn drop(&mut self) {
        let _ = self.core.detach_client(self.client_id);
    }
}

impl Drop for ServerConnectionClose {
    fn drop(&mut self) {
        self.inbox.close(None);
        let _ = self.shutdown.shutdown(Shutdown::Both);
    }
}

// The listener still owns the connection worker; its scoped reader is shut down
// and joined before that worker exits, including writer failure and unwind.
fn run_client_message_loop(
    stream: &mut TcpStream,
    stop_requested: &AtomicBool,
    context: &RfbServerContext,
    client_id: String,
    initial_display: SharedDisplay,
) -> Result<(), std::io::Error> {
    let view_only = context.view_only;
    let shared_display = Arc::clone(&context.shared_display);
    let frame_cache = Arc::clone(&context.frame_cache);
    let framebuffer_core = Arc::clone(&context.framebuffer_core);
    let mut read_stream = stream.try_clone()?;
    let relay = shared_display
        .read()
        .map_err(|_| std::io::Error::other("display poisoned"))?
        .relay
        .clone();
    read_stream.set_read_timeout(Some(RFB_IO_POLL))?;
    let inbox = Arc::new(ClientInbox::new(
        framebuffer_core
            .client_update_signal(&client_id)
            .map_err(std::io::Error::other)?,
    ));
    let last_sent_display = Arc::new(RwLock::new(None));
    let close = ServerConnectionClose {
        shutdown: Arc::new(stream.try_clone()?),
        inbox: Arc::clone(&inbox),
    };
    thread::scope(|scope| {
        let reader_close = ServerConnectionClose {
            shutdown: Arc::clone(&close.shutdown),
            inbox: Arc::clone(&inbox),
        };
        let reader_inbox = Arc::clone(&inbox);
        let reader_display = Arc::clone(&last_sent_display);
        let reader_cache = Arc::clone(&frame_cache);
        let reader = thread::Builder::new()
            .name("vnc-server-reader".into())
            .stack_size(512 * 1024)
            .spawn_scoped(scope, move || {
                let _close = reader_close;
                if let Err(error) = run_server_reader(
                    &mut read_stream,
                    stop_requested,
                    view_only,
                    &reader_display,
                    &reader_cache,
                    &reader_inbox,
                    relay,
                ) {
                    let graceful = matches!(
                        error.kind(),
                        std::io::ErrorKind::UnexpectedEof
                            | std::io::ErrorKind::ConnectionReset
                            | std::io::ErrorKind::ConnectionAborted
                            | std::io::ErrorKind::Interrupted
                    );
                    reader_inbox.close((!graceful).then_some(error));
                }
            })?;
        let result = run_server_writer(
            stream,
            stop_requested,
            context,
            client_id,
            initial_display,
            &inbox,
            &last_sent_display,
        );
        drop(close);
        reader
            .join()
            .map_err(|_| std::io::Error::other("server reader panicked"))?;
        result
    })
}

fn run_server_reader(
    stream: &mut TcpStream,
    stop_requested: &AtomicBool,
    view_only: bool,
    last_sent_display: &RwLock<Option<SharedDisplay>>,
    frame_cache: &LocalVncServerFrameCache,
    inbox: &ClientInbox,
    relay: Option<Arc<crate::local_vnc_server::desktop_relay::DesktopRelay>>,
) -> Result<(), std::io::Error> {
    let mut relay_input = relay.map(crate::local_vnc_server::desktop_relay::RelayInputOwner::new);
    let mut buttons = 0;
    let mut secure_attention = SecureAttentionKeyState::default();
    #[cfg(target_os = "windows")]
    let mut service_input = crate::local_vnc_server_platform::windows_desktop::is_service_worker()
        .then(crate::local_vnc_server_platform::windows_input::ServiceInputOwner::default);
    #[cfg(target_os = "windows")]
    let mut service_input_epoch = None;
    while !stop_requested.load(Ordering::SeqCst) && !inbox.closed.load(Ordering::SeqCst) {
        #[cfg(target_os = "windows")]
        if let Some(input) = service_input.as_mut() {
            let display = last_sent_display
                .read()
                .map_err(|_| std::io::Error::other("sent display poisoned"))?;
            prepare_service_input(
                input,
                display.as_ref(),
                &mut service_input_epoch,
                &mut secure_attention,
            );
        }
        let Some(message) = read_client_message(stream, stop_requested)? else {
            continue;
        };
        match message {
            ClientMessage::Key { down, keysym } => {
                if let Some(input) = relay_input.as_mut() {
                    if !view_only {
                        let display = last_sent_display
                            .read()
                            .map_err(|_| std::io::Error::other("sent display poisoned"))?;
                        input.key(display.as_ref(), down, keysym);
                    }
                    continue;
                }
                #[cfg(target_os = "windows")]
                if let Some(input) = service_input.as_mut() {
                    let display = last_sent_display
                        .read()
                        .map_err(|_| std::io::Error::other("sent display poisoned"))?
                        .clone();
                    if !view_only
                        && prepare_service_input(
                            input,
                            display.as_ref(),
                            &mut service_input_epoch,
                            &mut secure_attention,
                        )
                        && !try_handle_secure_attention_key_event(
                            down,
                            keysym,
                            &mut secure_attention,
                        )
                    {
                        input.key(down, keysym);
                    }
                    continue;
                }
                if !view_only
                    && !try_handle_secure_attention_key_event(down, keysym, &mut secure_attention)
                {
                    inject_key_event(down, keysym);
                }
            }
            ClientMessage::Pointer {
                buttons: next,
                x,
                y,
            } => {
                if !view_only {
                    let display = last_sent_display
                        .read()
                        .map_err(|_| std::io::Error::other("sent display poisoned"))?
                        .clone();
                    if let Some(display) = display {
                        if let Some(input) = relay_input.as_mut() {
                            input.pointer(&display, x, y, next);
                            continue;
                        }
                        #[cfg(target_os = "windows")]
                        if let Some(input) = service_input.as_mut() {
                            if prepare_service_input(
                                input,
                                Some(&display),
                                &mut service_input_epoch,
                                &mut secure_attention,
                            ) {
                                input.pointer(next, x, y, &display);
                            }
                            continue;
                        }
                        inject_pointer_event(next, x, y, &mut buttons, &display);
                    }
                }
            }
            message => {
                if matches!(message, ClientMessage::FrameRequest(_)) {
                    frame_cache.record_framebuffer_update_request();
                }
                if !inbox.push(message, stop_requested)? {
                    break;
                }
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn prepare_service_input(
    input: &mut crate::local_vnc_server_platform::windows_input::ServiceInputOwner,
    display: Option<&SharedDisplay>,
    previous_epoch: &mut Option<u64>,
    secure_attention: &mut SecureAttentionKeyState,
) -> bool {
    let ready = input.prepare(display);
    let next_epoch = display
        .filter(|_| ready)
        .map(|display| display.desktop_epoch);
    if *previous_epoch != next_epoch || !ready {
        *secure_attention = SecureAttentionKeyState::default();
        *previous_epoch = next_epoch;
    }
    ready
}
