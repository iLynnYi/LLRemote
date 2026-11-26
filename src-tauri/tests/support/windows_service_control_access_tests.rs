use super::*;
use windows_sys::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
use windows_sys::Win32::Security::{
    EqualSid, GetAce, GetSecurityDescriptorDacl, ACCESS_ALLOWED_ACE,
};
use windows_sys::Win32::System::Services::SERVICE_ALL_ACCESS;

const USER: &str = "S-1-5-21-10-20-30-1001";

fn descriptor_acl(sddl: &str) -> (LocalAllocation, *mut ACL) {
    let mut descriptor = null_mut();
    assert_ne!(
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                native::wide(sddl).as_ptr(),
                1,
                &mut descriptor,
                null_mut(),
            )
        },
        0
    );
    let owned = LocalAllocation(descriptor);
    let mut acl = null_mut();
    let mut present = 0;
    let mut defaulted = 0;
    assert_ne!(
        unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut acl, &mut defaulted) },
        0
    );
    assert_ne!(present, 0);
    (owned, acl)
}

fn allowed_mask(acl: *const ACL, user: &str) -> u32 {
    let user = sid(user).unwrap();
    let mut mask = 0;
    for index in 0..unsafe { (*acl).AceCount } {
        let mut entry = null_mut();
        assert_ne!(unsafe { GetAce(acl, u32::from(index), &mut entry) }, 0);
        let ace = entry.cast::<ACCESS_ALLOWED_ACE>();
        if unsafe { (*ace).Header.AceType } == 0 // ACCESS_ALLOWED_ACE_TYPE in winnt.h.
            && unsafe { EqualSid(std::ptr::addr_of_mut!((*ace).SidStart).cast(), user.0) } != 0
        {
            mask |= unsafe { (*ace).Mask };
        }
    }
    mask
}

#[test]
fn lifecycle_delegation_preserves_existing_roles_and_grants_only_control() {
    let (_descriptor, acl) = descriptor_acl("D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;CCLCSWLOCRRC;;;AU)");
    let user = sid(USER).unwrap();
    let merged = merge_control_acl(acl, &user).unwrap();
    let acl = merged.pointer();
    assert_eq!(
        allowed_mask(acl, USER),
        SERVICE_QUERY_STATUS | SERVICE_START | SERVICE_STOP
    );
    assert_eq!(allowed_mask(acl, "S-1-5-18"), 0x10000000); // Existing GENERIC_ALL remains intact.
    assert_eq!(allowed_mask(acl, "S-1-5-32-544"), 0x10000000);
    assert_eq!(
        allowed_mask(acl, "S-1-5-11") & (SERVICE_START | SERVICE_STOP),
        0
    );
    assert_ne!(allowed_mask(acl, USER), SERVICE_ALL_ACCESS);
}

#[test]
fn repeated_authorization_does_not_accumulate_entries_or_override_denials() {
    let (_descriptor, acl) = descriptor_acl(&format!("D:(D;;WP;;;{USER})(A;;GA;;;SY)(A;;GA;;;BA)"));
    let user = sid(USER).unwrap();
    let first = merge_control_acl(acl, &user).unwrap();
    let again = merge_control_acl(first.pointer(), &user).unwrap();
    let first_acl = first.pointer();
    let again_acl = again.pointer();
    assert_eq!(unsafe { (*again_acl).AceCount }, unsafe {
        (*first_acl).AceCount
    });
    assert_eq!(unsafe { (*again_acl).AclSize }, unsafe {
        (*first_acl).AclSize
    });
    assert_eq!(allowed_mask(again_acl, USER), CONTROL_RIGHTS);
    let mut entry = null_mut();
    assert_ne!(unsafe { GetAce(again_acl, 0, &mut entry) }, 0);
    assert_eq!(
        unsafe { (*entry.cast::<ACCESS_ALLOWED_ACE>()).Header.AceType },
        1
    );
    assert_eq!(
        unsafe { (*entry.cast::<ACCESS_ALLOWED_ACE>()).Mask },
        SERVICE_STOP
    );
}

#[test]
fn absent_acl_is_rejected_and_operator_identity_comes_from_the_process_token() {
    let user = sid(USER).unwrap();
    assert!(merge_control_acl(null_mut(), &user).is_err());
    assert_eq!(
        native::operator_user_sid(unsafe {
            windows_sys::Win32::System::Threading::GetCurrentProcessId()
        })
        .unwrap(),
        native::current_user_sid().unwrap(),
    );
    assert!(native::operator_user_sid(0).is_err());
}
