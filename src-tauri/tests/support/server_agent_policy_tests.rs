use super::*;

fn request_action(
    message_type: &str,
    view_only: bool,
    interactive: bool,
) -> Result<AgentResponse, String> {
    let session_id = Uuid::new_v4().to_string();
    let sessions = Arc::new(Mutex::new(HashMap::from([(
        session_id.clone(),
        Instant::now() + AGENT_SESSION_TTL,
    )])));
    let request = serde_json::from_value(serde_json::json!({
        "type": message_type,
        "sessionId": session_id,
    }))
    .unwrap();
    let shared_display = shared_display_for_agent_request_from_inventory(vec![], None, None);
    handle_agent_request(
        request,
        &AgentBootstrap {
            key: Uuid::new_v4().to_string(),
            tickets: Arc::new(Mutex::new(HashMap::new())),
        },
        "policy-fixture",
        &sessions,
        &Arc::new(Mutex::new(HashMap::new())),
        Path::new(""),
        AgentRuntimeContext {
            shared_display: Arc::new(RwLock::new(shared_display)),
            view_only,
            interactive: Some(Arc::new(AtomicBool::new(interactive))),
        },
    )
}

#[test]
fn view_only_agent_rejects_file_writes_and_open_actions() {
    for action in [
        FILE_TRANSFER_CREATE,
        FILE_TRANSFER_CHUNK,
        FILE_OPEN,
        FILE_REVEAL,
        MESSAGE_SHOW,
    ] {
        assert_eq!(
            request_action(action, true, true).unwrap_err(),
            "agent_view_only"
        );
    }
}

#[test]
fn view_only_agent_can_select_which_display_to_view() {
    let description = request_action("agent.session.describe", true, true)
        .unwrap()
        .description
        .unwrap();
    assert!(description
        .capabilities
        .iter()
        .any(|capability| capability == AGENT_DISPLAY_SELECT));
    assert_eq!(
        request_action(AGENT_DISPLAY_SELECT, true, true).unwrap_err(),
        "agent_display_id_required"
    );
    assert_eq!(
        request_action(AGENT_DISPLAY_SELECT, true, false).unwrap_err(),
        "agent_user_desktop_unavailable"
    );
}

#[test]
fn inactive_user_desktop_rejects_clipboard_and_desktop_actions_before_native_calls() {
    for action in [
        CLIPBOARD_TEXT_READ,
        CLIPBOARD_TEXT_WRITE,
        MESSAGE_SHOW,
        FILE_OPEN,
        AGENT_DISPLAY_SELECT,
        SESSION_END,
    ] {
        assert_eq!(
            request_action(action, false, false).unwrap_err(),
            "agent_user_desktop_unavailable"
        );
    }
}

#[test]
fn authenticated_description_remains_available_without_interactive_desktop() {
    let response = request_action("agent.session.describe", true, false).unwrap();
    assert!(response.ok);
    let description = response.description.unwrap();
    assert!(description.view_only);
    assert!(!description
        .capabilities
        .iter()
        .any(|capability| capability == CLIPBOARD_TEXT_READ || capability == FILE_OPEN));
}

#[test]
fn message_capability_requires_control_and_an_interactive_supported_platform() {
    for (view_only, interactive) in [(false, true), (true, true), (false, false)] {
        let description = request_action("agent.session.describe", view_only, interactive)
            .unwrap()
            .description
            .unwrap();
        let supported = description.capabilities.iter().any(|id| id == MESSAGE_SHOW);
        assert_eq!(
            supported,
            !view_only && interactive && cfg!(any(target_os = "macos", target_os = "windows"))
        );
    }
    // Missing text reaches validation only after authentication and policy checks.
    assert_eq!(
        request_action(MESSAGE_SHOW, false, true).unwrap_err(),
        "remote_message_empty"
    );
}

#[test]
fn authenticated_activity_renews_an_agent_session_near_expiry() {
    let session_id = Uuid::new_v4().to_string();
    let now = Instant::now();
    let sessions = Arc::new(Mutex::new(HashMap::from([(
        session_id.clone(),
        now + Duration::from_secs(1),
    )])));
    let request = serde_json::from_value(serde_json::json!({
        "type": "agent.session.describe", "sessionId": session_id,
    }))
    .unwrap();
    let response = handle_agent_request(
        request,
        &AgentBootstrap {
            key: Uuid::new_v4().to_string(),
            tickets: Arc::new(Mutex::new(HashMap::new())),
        },
        "session-fixture",
        &sessions,
        &Arc::new(Mutex::new(HashMap::new())),
        Path::new(""),
        AgentRuntimeContext {
            shared_display: Arc::new(RwLock::new(
                shared_display_for_agent_request_from_inventory(vec![], None, None),
            )),
            view_only: true,
            interactive: None,
        },
    )
    .unwrap();
    assert!(response.ok);
    assert!(sessions.lock().unwrap()[&session_id] >= now + Duration::from_secs(30));
}

#[test]
fn session_end_notification_is_available_to_viewers_and_removed_without_user_desktop() {
    for (view_only, interactive) in [(false, false), (true, false)] {
        let description = request_action("agent.session.describe", view_only, interactive)
            .unwrap()
            .description
            .unwrap();
        assert!(!description.capabilities.iter().any(|id| id == SESSION_END));
    }
    let description = request_action("agent.session.describe", true, true)
        .unwrap()
        .description
        .unwrap();
    assert_eq!(
        description.capabilities.iter().any(|id| id == SESSION_END),
        cfg!(any(target_os = "macos", target_os = "windows"))
    );
}
