use super::*;
use crate::local_vnc_server::{
    DisplayFrame, LocalVncServerRefreshMode, LocalVncServerResolutionMode,
};
use std::sync::atomic::AtomicUsize;
use std::sync::{mpsc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

fn listener_fixture(
    changed: Option<Arc<dyn Fn() + Send + Sync>>,
) -> (u16, Arc<AtomicBool>, JoinHandle<()>, mpsc::Receiver<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let (exited, exit) = mpsc::channel();
    let worker = thread::spawn(move || {
        run_listener(
            listener,
            worker_stop,
            Arc::new(LocalVncServerConnectionRegistry::new(changed)),
            RfbServerContext {
                auth_key: [0; 8],
                view_only: false,
                shared_display: Arc::new(RwLock::new(SharedDisplay {
                    relay: None,
                    revision: 0,
                    desktop_epoch: 0,
                    id: "fixture-display".into(),
                    label: "Fixture".into(),
                    frame: DisplayFrame {
                        x: 0,
                        y: 0,
                        width: 2,
                        height: 1,
                        pixel_width: 2,
                        pixel_height: 1,
                    },
                    framebuffer: DisplaySize {
                        width: 2,
                        height: 1,
                    },
                    resolution_mode: LocalVncServerResolutionMode::Native,
                    refresh_mode: LocalVncServerRefreshMode::Responsive,
                })),
                frame_cache: Arc::new(LocalVncServerFrameCache::default()),
                framebuffer_core: Arc::new(
                    crate::remote_desktop_core::ServerFramebufferCore::default(),
                ),
                discovery: crate::server_discovery::ServerDiscoveryContext::new(
                    "fixture-server".into(),
                    false,
                    None,
                    None,
                ),
            },
        );
        let _ = exited.send(());
    });
    (port, stop, worker, exit)
}

fn silent_peer(port: u16) -> TcpStream {
    let mut peer = TcpStream::connect(("127.0.0.1", port)).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let mut banner = [0; 12];
    peer.read_exact(&mut banner).unwrap();
    assert_eq!(&banner, RFB_VERSION);
    peer
}

#[test]
fn listener_cannot_finish_while_a_client_worker_is_still_releasing_its_resources() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (entered, enter) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let released = Mutex::new(released);
    let changed = Arc::new(move || {
        if calls.fetch_add(1, Ordering::SeqCst) == 1 {
            let _ = entered.send(());
            let _ = released
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5));
        }
    });
    let (port, stop, listener, exit) = listener_fixture(Some(changed));
    drop(silent_peer(port));
    enter
        .recv_timeout(Duration::from_secs(2))
        .expect("client reached resource release");
    stop.store(true, Ordering::SeqCst);
    let premature_exit = exit.recv_timeout(Duration::from_millis(300)).is_ok();
    release.send(()).unwrap();
    listener.join().unwrap();
    assert!(
        !premature_exit,
        "listener returned with a live client worker"
    );
}

#[test]
fn rfb_admission_is_bounded_and_finished_workers_release_their_slots() {
    let (port, stop, listener, _exit) = listener_fixture(None);
    let mut peers = (0..8).map(|_| silent_peer(port)).collect::<Vec<_>>();
    let mut overflow = TcpStream::connect(("127.0.0.1", port)).unwrap();
    overflow
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut byte = [0];
    let rejected = matches!(overflow.read(&mut byte), Ok(0));
    drop(overflow);
    drop(peers.remove(0));
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut reused = false;
    while Instant::now() < deadline {
        let mut candidate = TcpStream::connect(("127.0.0.1", port)).unwrap();
        candidate
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let mut banner = [0; 12];
        if candidate.read_exact(&mut banner).is_ok() {
            assert_eq!(&banner, RFB_VERSION);
            peers.push(candidate);
            reused = true;
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    stop.store(true, Ordering::SeqCst);
    listener.join().unwrap();
    assert!(rejected, "ninth silent RFB peer received a worker");
    assert!(
        reused,
        "finished RFB worker did not release its admission slot"
    );
}
