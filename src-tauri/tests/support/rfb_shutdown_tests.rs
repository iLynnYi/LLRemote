use super::*;

fn request(id: &str) -> RemoteSessionStartRequest {
    serde_json::from_value(serde_json::json!({
        "sessionId": id, "target": "shutdown-fixture",
        "displaySource": "vnc", "inputSource": "vnc",
        "connectionModel": {
            "baseTransport": { "kind": "vnc" },
            "serverIdentity": { "kind": "unknown", "verification": "none" },
            "localEndpointMatch": { "state": "not-matched" },
            "enhancedChannel": { "status": "unavailable", "authentication": "not-attempted" },
            "capabilities": { "items": [] }
        }
    }))
    .unwrap()
}

#[test]
fn disconnect_cannot_reopen_a_session_claimed_by_full_stop() {
    let manager = RemoteSessionManager::default();
    let id = "shutdown-gate";
    manager.start(request(id)).unwrap();
    // Interleave the production claim/join/commit phases: full stop owns the old
    // Surface, while the competing disconnect completes its own TCP teardown.
    let stopping = manager.begin_rfb_shutdown(id, true).unwrap().unwrap();
    manager.disconnect_rfb(id).unwrap();
    let state = manager.get_snapshot(id).unwrap().unwrap().state;
    let reconnect = manager.connect_rfb(
        RemoteSessionRfbConnectRequest {
            session_id: id.into(),
            host: "127.0.0.1".into(),
            port: 0,
            password: None,
        },
        |_| {},
    );
    stopping.join().unwrap();
    let stopped = manager.finish_rfb_shutdown(stopping);
    manager.stop(id).unwrap();
    assert_eq!(state, "stopping");
    assert_eq!(reconnect.unwrap_err(), "rfb_cancelled");
    stopped.unwrap();
}

#[test]
fn an_old_stop_completion_cannot_remove_a_replacement_with_the_same_id() {
    let manager = RemoteSessionManager::default();
    let id = "shutdown-replacement";
    manager.start(request(id)).unwrap();
    let first = manager.begin_rfb_shutdown(id, true).unwrap().unwrap();
    let second = manager.begin_rfb_shutdown(id, true).unwrap().unwrap();
    first.join().unwrap();
    manager.finish_rfb_shutdown(first).unwrap();
    manager.start(request(id)).unwrap();
    // Both Core instances begin with generation zero; identity must protect the
    // replacement even though the old teardown has the same numeric generation.
    second.join().unwrap();
    manager.finish_rfb_shutdown(second).unwrap();
    let replacement = manager.get_snapshot(id).unwrap();
    manager.stop(id).unwrap();
    assert_eq!(replacement.unwrap().state, "starting");
}

#[test]
fn an_old_startup_failure_cannot_fail_a_replacement_with_the_same_generation() {
    let manager = RemoteSessionManager::default();
    let id = "startup-replacement";
    manager.start(request(id)).unwrap();
    // Capture the first generation as connect_rfb does, then delay its failure
    // commit until a full stop and a same-ID replacement have both completed.
    let (framebuffer, generation) = {
        let mut sessions = manager.sessions.lock().unwrap();
        let session = sessions.get_mut(id).unwrap();
        session.rfb_generation = 1;
        (Arc::clone(&session.framebuffer), session.rfb_generation)
    };
    manager.stop(id).unwrap();
    manager.start(request(id)).unwrap();
    {
        let mut sessions = manager.sessions.lock().unwrap();
        let session = sessions.get_mut(id).unwrap();
        session.rfb_generation = generation;
        session.state = "connecting".into();
        assert!(!Arc::ptr_eq(&session.framebuffer, &framebuffer));
    }
    manager.clear_failed_rfb_start(id, generation, &framebuffer, "rfb_cancelled");
    let (state, failure) = {
        let sessions = manager.sessions.lock().unwrap();
        let session = sessions.get(id).unwrap();
        let failure = session.rfb_failure.lock().unwrap().clone();
        (session.state.clone(), failure)
    };
    manager.stop(id).unwrap();
    assert_eq!(state, "connecting");
    assert_eq!(failure, None);
}
