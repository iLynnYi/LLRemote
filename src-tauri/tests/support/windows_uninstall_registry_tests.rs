use super::*;
use windows_sys::Win32::System::Registry::{
    RegCreateKeyExW, RegSetValueExW, HKEY_CURRENT_USER, REG_OPTION_NON_VOLATILE, REG_SZ,
};

fn create(root: HKEY, path: &str) -> Key {
    let mut key = null_mut();
    assert_eq!(
        unsafe {
            RegCreateKeyExW(
                root,
                native::wide(path).as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_READ | KEY_WRITE,
                std::ptr::null(),
                &mut key,
                std::ptr::null_mut(),
            )
        },
        0
    );
    Key(key, None)
}

fn set(root: HKEY, name: &str, value: &str) {
    let value = native::wide(value);
    assert_eq!(
        unsafe {
            RegSetValueExW(
                root,
                native::wide(name).as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            )
        },
        0
    );
}

#[test]
fn uninstall_registration_is_matched_and_removed_only_in_the_supplied_owner_hive() {
    // Both "hives" are disposable subkeys. No actual install/profile/service
    // or credential is touched, even when this test runs elevated.
    let path = format!(
        "Software\\LLRemote.Uninstall.Tests\\{}",
        uuid::Uuid::new_v4()
    );
    let root = create(HKEY_CURRENT_USER, &path);
    let alice = create(root.0, "Alice");
    let bob = create(root.0, "Bob");
    let alice_install = create(alice.0, UNINSTALL_KEY);
    let bob_install = create(bob.0, UNINSTALL_KEY);
    set(
        alice_install.0,
        "UninstallString",
        r#""C:\Fixture\Alice\uninstall.exe""#,
    );
    set(
        bob_install.0,
        "UninstallString",
        r#""C:\Fixture\Bob\uninstall.exe""#,
    );
    assert!(registered_at(alice.0, Path::new(r"C:\Fixture\Alice")).unwrap());
    assert!(!registered_at(bob.0, Path::new(r"C:\Fixture\Alice")).unwrap());
    drop(alice_install);
    drop(bob_install);
    delete_key(alice.0, UNINSTALL_KEY).unwrap();
    assert!(!registered_at(alice.0, Path::new(r"C:\Fixture\Alice")).unwrap());
    assert!(registered_at(bob.0, Path::new(r"C:\Fixture\Bob")).unwrap());
    drop(alice);
    drop(bob);
    drop(root);
    delete_key(HKEY_CURRENT_USER, &path).unwrap();
}
