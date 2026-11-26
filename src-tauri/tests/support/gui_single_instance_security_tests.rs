use super::*;
use std::ptr::null_mut;
use windows_sys::Win32::{
    Foundation::{LocalFree, ERROR_ACCESS_DENIED},
    Security::{
        Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT},
        GetAce, GetSidSubAuthority, GetSidSubAuthorityCount, LABEL_SECURITY_INFORMATION,
        SYSTEM_MANDATORY_LABEL_ACE,
    },
    System::{
        SystemServices::{
            SECURITY_MANDATORY_MEDIUM_RID, SYSTEM_MANDATORY_LABEL_ACE_TYPE,
            SYSTEM_MANDATORY_LABEL_NO_WRITE_UP,
        },
        Threading::{OpenEventW, EVENT_ALL_ACCESS, SYNCHRONIZATION_READ_CONTROL},
    },
};

#[test]
fn wake_event_allows_signaling_without_granting_full_control() {
    let sid = native::current_user_sid().unwrap();
    let key = scope(
        &format!("com.llremote.test.{}", uuid::Uuid::new_v4().simple()),
        &sid,
    );
    let wake = create_wake(&key, &sid).unwrap();
    let name = native::wide(format!("{key}-wake"));

    let wait =
        Handle::new(unsafe { OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, 0, name.as_ptr()) }).unwrap();
    assert_eq!(unsafe { SetEvent(wait.0 as _) }, 0);
    assert_eq!(unsafe { GetLastError() }, ERROR_ACCESS_DENIED);

    let signal = Handle::new(unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr()) }).unwrap();
    assert_ne!(unsafe { SetEvent(signal.0 as _) }, 0);
    assert_eq!(
        unsafe { WaitForSingleObject(wake.0 as _, 1000) },
        WAIT_OBJECT_0
    );

    let full_access = Handle::new(unsafe { OpenEventW(EVENT_ALL_ACCESS, 0, name.as_ptr()) });
    assert_eq!(
        full_access.err().unwrap().raw_os_error(),
        Some(ERROR_ACCESS_DENIED as i32)
    );
}

#[test]
fn wake_event_has_a_medium_integrity_label_even_when_the_gui_is_elevated() {
    let sid = native::current_user_sid().unwrap();
    let key = scope(
        &format!("com.llremote.test.{}", uuid::Uuid::new_v4().simple()),
        &sid,
    );
    let _wake = create_wake(&key, &sid).unwrap();
    let read = Handle::new(unsafe {
        OpenEventW(
            SYNCHRONIZATION_READ_CONTROL,
            0,
            native::wide(format!("{key}-wake")).as_ptr(),
        )
    })
    .unwrap();
    let mut sacl = null_mut();
    let mut descriptor = null_mut();
    assert_eq!(
        unsafe {
            GetSecurityInfo(
                read.0 as _,
                SE_KERNEL_OBJECT,
                LABEL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                null_mut(),
                &mut sacl,
                &mut descriptor,
            )
        },
        0
    );
    struct FreeDescriptor(*mut std::ffi::c_void);
    impl Drop for FreeDescriptor {
        fn drop(&mut self) {
            unsafe { LocalFree(self.0) };
        }
    }
    let _descriptor = FreeDescriptor(descriptor);
    assert!(
        !sacl.is_null(),
        "kernel object is missing its explicit integrity label"
    );
    let mut ace = null_mut();
    assert_ne!(unsafe { GetAce(sacl, 0, &mut ace) }, 0);
    let ace = unsafe { &*ace.cast::<SYSTEM_MANDATORY_LABEL_ACE>() };
    assert_eq!(
        u32::from(ace.Header.AceType),
        SYSTEM_MANDATORY_LABEL_ACE_TYPE
    );
    assert_eq!(ace.Mask, SYSTEM_MANDATORY_LABEL_NO_WRITE_UP);
    let sid = std::ptr::addr_of!(ace.SidStart).cast_mut().cast();
    let count = unsafe { *GetSidSubAuthorityCount(sid) };
    assert!(count > 0);
    assert_eq!(
        unsafe { *GetSidSubAuthority(sid, u32::from(count - 1)) },
        SECURITY_MANDATORY_MEDIUM_RID as u32
    );
}
