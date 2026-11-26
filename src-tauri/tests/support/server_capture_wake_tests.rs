#[test]
fn capture_published_after_check_but_before_wait_wakes_the_writer() {
    use crate::remote_desktop_core::{
        Framebuffer, FramebufferGeometry, PixelFormat, ServerFramebufferCore,
    };
    let core = ServerFramebufferCore::default();
    core.attach_client("wake-client").unwrap();
    let inbox = ClientInbox::new(core.client_update_signal("wake-client").unwrap());
    let observed = inbox.updates.version();
    core.publish(
        Framebuffer::new(
            FramebufferGeometry {
                width: 2,
                height: 1,
                pixel_format: PixelFormat::Bgra8888,
            },
            vec![7; 8],
        )
        .unwrap(),
    )
    .unwrap();
    let started = Instant::now();
    assert!(matches!(
        inbox.next_after(observed, Duration::from_secs(2)).unwrap(),
        ClientInboxItem::Idle
    ));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "a prior capture notification must prevent sleeping for the whole wait timeout"
    );
}

#[test]
fn a_waiting_writer_is_woken_by_capture_and_control_and_close() {
    use crate::remote_desktop_core::{
        Framebuffer, FramebufferGeometry, PixelFormat, ServerFramebufferCore,
    };
    let core = ServerFramebufferCore::default();
    core.attach_client("waiting-client").unwrap();
    let inbox = Arc::new(ClientInbox::new(
        core.client_update_signal("waiting-client").unwrap(),
    ));
    let observed = inbox.updates.version();
    let worker_inbox = Arc::clone(&inbox);
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        entered_tx.send(()).unwrap();
        done_tx
            .send(matches!(
                worker_inbox
                    .next_after(observed, Duration::from_secs(5))
                    .unwrap(),
                ClientInboxItem::Idle
            ))
            .unwrap();
    });
    entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    core.publish(
        Framebuffer::new(
            FramebufferGeometry {
                width: 2,
                height: 1,
                pixel_format: PixelFormat::Bgra8888,
            },
            vec![7; 8],
        )
        .unwrap(),
    )
    .unwrap();
    let woken = done_rx.recv_timeout(Duration::from_secs(1));
    inbox.updates.notify();
    worker.join().unwrap();
    assert!(woken.expect("capture must wake the waiting writer"));

    let observed = inbox.updates.version();
    inbox
        .push(ClientMessage::PixelFormat([0; 19]), &AtomicBool::new(false))
        .unwrap();
    assert!(matches!(
        inbox.next_after(observed, Duration::from_secs(5)).unwrap(),
        ClientInboxItem::Message(ClientMessage::PixelFormat(_))
    ));
    let observed = inbox.updates.version();
    inbox.close(None);
    assert!(matches!(
        inbox.next_after(observed, Duration::from_secs(5)).unwrap(),
        ClientInboxItem::Closed
    ));
}

#[test]
fn a_replaced_attachment_has_a_new_signal_and_static_captures_do_not_wake() {
    use crate::remote_desktop_core::{
        Framebuffer, FramebufferGeometry, PixelFormat, ServerFramebufferCore,
    };
    let core = ServerFramebufferCore::default();
    core.attach_client("client").unwrap();
    let old = core.client_update_signal("client").unwrap();
    core.detach_client("client").unwrap();
    let old_version = old.version();
    core.attach_client("client").unwrap();
    let current = core.client_update_signal("client").unwrap();
    assert!(!Arc::ptr_eq(&old, &current));
    let geometry = FramebufferGeometry {
        width: 2,
        height: 1,
        pixel_format: PixelFormat::Bgra8888,
    };
    let publish = |revision| {
        core.publish_for_source(Framebuffer::new(geometry, vec![7; 8]).unwrap(), revision)
            .unwrap()
    };
    publish(0);
    let version = current.version();
    assert_eq!(old.version(), old_version);
    publish(0);
    assert_eq!(
        current.version(),
        version,
        "identical pixels must not trigger writer polling"
    );
    publish(1);
    assert_ne!(
        current.version(),
        version,
        "same-size source changes must wake the writer"
    );
}

#[test]
fn connection_unwind_releases_core_notification_ownership() {
    use crate::remote_desktop_core::ServerFramebufferCore;
    let core = ServerFramebufferCore::default();
    core.attach_client("owner").unwrap();
    let signal = core.client_update_signal("owner").unwrap();
    let weak = Arc::downgrade(&signal);
    drop(signal);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _attachment = ServerClientAttachment {
            core: &core,
            client_id: "owner",
        };
        panic!("connection fixture unwind");
    }));
    assert!(panic.is_err());
    assert!(weak.upgrade().is_none());
    core.attach_client("owner").unwrap();
}
