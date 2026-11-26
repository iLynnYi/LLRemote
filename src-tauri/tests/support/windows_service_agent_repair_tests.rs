use super::*;

#[test]
fn delegated_daily_control_does_not_authorize_enhancement_repair() {
    let mut supervisor = Supervisor::new();
    supervisor.user_agent_error = Some("windows_service_user_agent_identity_failed".into());
    let reply = supervisor.handle(
        Request::RepairUserAgent,
        Peer {
            pid: 123,
            session_id: 8,
            system: false,
            elevated: false,
            user_sid: "S-1-5-21-10-20-30-1001".into(),
        },
    );
    assert_eq!(
        reply.error_code.as_deref(),
        Some("windows_service_admin_required")
    );
    assert_eq!(
        supervisor.user_agent_error.as_deref(),
        Some("windows_service_user_agent_identity_failed")
    );
}

#[test]
fn enhancement_repair_without_a_ready_desktop_worker_does_not_claim_recovery() {
    let mut supervisor = Supervisor::new();
    let reply = supervisor.handle(
        Request::RepairUserAgent,
        Peer {
            pid: 123,
            session_id: 8,
            system: false,
            elevated: true,
            user_sid: "S-1-5-21-10-20-30-1001".into(),
        },
    );
    assert_eq!(
        reply.error_code.as_deref(),
        Some("windows_service_user_agent_unavailable")
    );
    assert!(supervisor.user_agent.is_none());
    assert!(!supervisor.broker.is_running());
}
