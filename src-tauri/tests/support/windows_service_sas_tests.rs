use super::*;
use windows_sys::Win32::System::Registry::{
    RegDeleteTreeW, HKEY_CURRENT_USER, REG_OPTION_VOLATILE,
};

struct Fixture {
    path: String,
    key: PolicyKey,
}

impl Fixture {
    fn new() -> Self {
        let path = format!(r"Software\LLRemoteSasTest-{}", uuid::Uuid::new_v4());
        let mut key = null_mut();
        assert_eq!(
            unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    native::wide(&path).as_ptr(),
                    0,
                    null(),
                    REG_OPTION_VOLATILE,
                    KEY_QUERY_VALUE | KEY_SET_VALUE,
                    null(),
                    &mut key,
                    null_mut(),
                )
            },
            0
        );
        Self {
            path,
            key: PolicyKey(key),
        }
    }

    fn set(&self, value: u32) {
        assert_eq!(
            unsafe {
                RegSetValueExW(
                    self.key.0,
                    native::wide(POLICY_VALUE).as_ptr(),
                    0,
                    REG_DWORD,
                    (&value as *const u32).cast(),
                    4,
                )
            },
            0
        );
    }

    fn value(&self) -> u32 {
        let mut value = 0_u32;
        let mut size = 4;
        assert_eq!(
            unsafe {
                RegGetValueW(
                    self.key.0,
                    null(),
                    native::wide(POLICY_VALUE).as_ptr(),
                    RRF_RT_REG_DWORD,
                    null_mut(),
                    (&mut value as *mut u32).cast(),
                    &mut size,
                )
            },
            0
        );
        value
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, native::wide(&self.path).as_ptr()) };
    }
}

#[test]
fn unconfigured_policy_can_allow_only_services() {
    let fixture = Fixture::new();
    allow_services_for_key(fixture.key.0).unwrap();
    assert_eq!(fixture.value(), 1);
}

#[test]
fn configured_denial_is_not_overwritten() {
    for value in [0, 2] {
        let fixture = Fixture::new();
        fixture.set(value);
        assert_eq!(
            allow_services_for_key(fixture.key.0).unwrap_err(),
            "windows_service_sas_policy_denied"
        );
        assert_eq!(fixture.value(), value);
    }
}

#[test]
fn configured_allowed_policy_keeps_its_scope() {
    for value in [1, 3] {
        let fixture = Fixture::new();
        fixture.set(value);
        allow_services_for_key(fixture.key.0).unwrap();
        assert_eq!(fixture.value(), value);
    }
}

#[test]
fn unknown_policy_is_not_overwritten() {
    let fixture = Fixture::new();
    fixture.set(7);
    assert_eq!(
        allow_services_for_key(fixture.key.0).unwrap_err(),
        "windows_service_sas_policy_configure_failed"
    );
    assert_eq!(fixture.value(), 7);
}

#[test]
fn explicitly_repairing_policy_changes_denials_without_removing_uiaccess() {
    for (previous, expected) in [(0, 1), (2, 3), (1, 1), (3, 3)] {
        let fixture = Fixture::new();
        fixture.set(previous);
        configure_key(fixture.key.0, true).unwrap();
        assert_eq!(fixture.value(), expected);
    }
}
