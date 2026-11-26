use super::*;
use std::cell::RefCell;

#[test]
fn requesting_a_permission_never_opens_settings_even_when_not_yet_granted() {
    for kind in [
        LocalVncServerPermissionKind::Accessibility,
        LocalVncServerPermissionKind::ScreenRecording,
    ] {
        let effects = RefCell::new(Vec::new());
        let result = perform_macos_permission_action(
            kind.clone(),
            false,
            |_| {
                effects.borrow_mut().push("prompt");
                false // Prompt started, but a trust check is still false until approval.
            },
            |kind, target| {
                effects.borrow_mut().push("settings");
                Ok(LocalVncServerPermissionRequestResult {
                    opened: true,
                    kind,
                    platform: "macos".into(),
                    target: target.into(),
                    message: String::new(),
                })
            },
        )
        .unwrap();
        assert_eq!(*effects.borrow(), ["prompt"]);
        assert_eq!(result.kind, kind);
        assert!(!result.target.starts_with("x-apple."));
    }
}

#[test]
fn explicitly_opening_settings_does_not_also_trigger_a_prompt() {
    let result = perform_macos_permission_action(
        LocalVncServerPermissionKind::Accessibility,
        true,
        |_| panic!("explicit settings must not open a second system prompt"),
        |kind, target| {
            assert!(target.ends_with("Privacy_Accessibility"));
            Ok(LocalVncServerPermissionRequestResult {
                opened: true,
                kind,
                platform: "macos".into(),
                target: target.into(),
                message: String::new(),
            })
        },
    )
    .unwrap();
    assert!(result.opened);
}

#[test]
fn overview_requires_supported_permission_kinds_and_states() {
    use crate::local_vnc_server::{pending_local_vnc_server_overview, LocalVncServerOverview};

    let mut value = serde_json::to_value(pending_local_vnc_server_overview()).unwrap();
    value["permissions"] = serde_json::json!([
        { "kind": "screen_recording", "status": "granted", "requiredFor": "screen_capture" }
    ]);
    let overview: LocalVncServerOverview = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        overview.permissions[0].kind,
        LocalVncServerPermissionKind::ScreenRecording
    );

    let mut invalid_kind = value.clone();
    invalid_kind["permissions"][0]["kind"] = serde_json::json!("retired_network_permission");
    assert!(serde_json::from_value::<LocalVncServerOverview>(invalid_kind).is_err());

    value["permissions"][0]["status"] = serde_json::json!("invalid_status");
    assert!(serde_json::from_value::<LocalVncServerOverview>(value).is_err());
}
