use super::*;

const USER: &str = "S-1-5-21-10-20-30-1001";

#[test]
fn elevated_app_can_launch_from_the_same_users_ordinary_desktop() {
    let caller = UserContext {
        sid: USER,
        session: 8,
        elevated: true,
    };
    let desktop = UserContext {
        sid: USER,
        session: 8,
        elevated: false,
    };
    assert_eq!(
        launch_mode(&caller, &desktop).unwrap(),
        LaunchMode::InheritDesktop
    );
}

#[test]
fn elevated_desktop_uses_a_restricted_token_without_requiring_a_service_or_uac_change() {
    let desktop = UserContext {
        sid: USER,
        session: 8,
        elevated: true,
    };
    for elevated in [false, true] {
        let caller = UserContext {
            sid: USER,
            session: 8,
            elevated,
        };
        assert_eq!(
            launch_mode(&caller, &desktop).unwrap(),
            LaunchMode::RestrictedToken
        );
    }
}

#[test]
fn another_user_another_session_and_system_are_still_rejected() {
    let caller = UserContext {
        sid: USER,
        session: 8,
        elevated: false,
    };
    for (sid, session) in [("S-1-5-21-10-20-30-1002", 8), (USER, 9), (USER, 0)] {
        let desktop = UserContext {
            sid,
            session,
            elevated: false,
        };
        assert!(launch_mode(&caller, &desktop).is_err());
    }
    for sid in [USER, "S-1-5-18"] {
        let context = UserContext {
            sid,
            session: 0,
            elevated: false,
        };
        assert!(launch_mode(&context, &context).is_err());
    }
    let system = UserContext {
        sid: "S-1-5-18",
        session: 8,
        elevated: true,
    };
    assert!(launch_mode(&system, &system).is_err());
}
