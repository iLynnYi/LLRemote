use super::*;

const ALICE: &str = "S-1-5-21-1-2-3-1001";
const BOB: &str = "S-1-5-21-1-2-3-1002";

#[test]
fn personal_install_with_an_associated_service_requires_early_authorization() {
    let install = Installation::user(ALICE.into());
    let ordinary = install.plan(ALICE, false, None).unwrap();
    assert!(!ordinary.needs_elevation);
    assert!(ordinary.service_needs_elevation(true, false));
    assert!(!ordinary.service_needs_elevation(false, false));
    let authorized = install.plan(BOB, true, Some(ALICE)).unwrap();
    assert_eq!(authorized.user_sid, ALICE);
    assert!(!authorized.service_needs_elevation(true, true));
}

#[test]
fn user_install_always_targets_its_owner_even_when_an_admin_runs_it() {
    let install = Installation::user(ALICE.into());
    let plan = install.plan(BOB, true, None).unwrap();
    assert_eq!(plan.user_sid, ALICE);
    assert!(!plan.needs_elevation);
    assert!(!plan.machine);
    assert!(install.plan(BOB, true, Some(BOB)).is_err());
}

#[test]
fn another_user_must_elevate_before_any_cleanup() {
    let plan = Installation::user(ALICE.into())
        .plan(BOB, false, None)
        .unwrap();
    assert_eq!(plan.user_sid, ALICE);
    assert!(plan.needs_elevation);
    assert!(
        !Installation::user(ALICE.into())
            .plan(ALICE, false, None)
            .unwrap()
            .needs_elevation
    );
}

#[test]
fn machine_relaunch_keeps_the_original_operator_instead_of_the_authorizing_admin() {
    let install = Installation::machine();
    let initial = install.plan(ALICE, false, None).unwrap();
    assert!(initial.needs_elevation);
    let resumed = install.plan(BOB, true, Some(&initial.user_sid)).unwrap();
    assert!(resumed.machine);
    assert_eq!(resumed.user_sid, ALICE);
    assert!(!resumed.needs_elevation);
    assert!(
        install
            .plan(BOB, false, Some(ALICE))
            .unwrap()
            .needs_elevation
    );
}

#[test]
fn missing_or_invalid_ownership_never_falls_back_to_the_admins_profile() {
    assert!(Installation::parse("").is_err());
    assert!(Installation::parse("1\nuser\n").is_err());
    assert!(Installation::parse("1\nuser\nS-1-5-18\n").is_err());
    assert!(Installation::parse("1\nall\n\nextra").is_err());
    assert!(Installation::machine()
        .plan(ALICE, true, Some("../Bob"))
        .is_err());
    let value = Installation::user(ALICE.into());
    assert_eq!(Installation::parse(&value.encode()).unwrap(), value);
    assert_eq!(
        Installation::parse(&Installation::machine().encode()).unwrap(),
        Installation::machine()
    );
}

#[test]
fn administrator_delegates_to_the_owners_session_instead_of_copying_its_token() {
    assert_eq!(
        CleanupContext::select(ALICE, BOB, true, true).unwrap(),
        CleanupContext::OwnerSession
    );
    assert_eq!(
        CleanupContext::select(ALICE, ALICE, false, true).unwrap(),
        CleanupContext::CurrentUser
    );
    assert_eq!(
        CleanupContext::select(ALICE, BOB, false, false).unwrap(),
        CleanupContext::Offline
    );
    assert_eq!(
        CleanupContext::select(ALICE, BOB, false, true).unwrap(),
        CleanupContext::Offline
    );
}
