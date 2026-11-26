use super::*;
use windows_sys::Win32::Security::{TOKEN_DUPLICATE, TOKEN_QUERY};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

fn default_dacl(token: HANDLE) -> Vec<u8> {
    let data = buffer(token, TokenDefaultDacl, "test_default_dacl").unwrap();
    let dacl = unsafe { &*data.as_ptr().cast::<TOKEN_DEFAULT_DACL>() };
    assert!(!dacl.DefaultDacl.is_null());
    let size = unsafe { (*dacl.DefaultDacl).AclSize };
    unsafe { std::slice::from_raw_parts(dacl.DefaultDacl.cast::<u8>(), size as usize) }.to_vec()
}

#[test]
fn restricted_default_dacl_grants_self_process_access_without_changing_the_source_token() {
    use windows_sys::Win32::Security::{
        AccessCheck, InitializeSecurityDescriptor, SetSecurityDescriptorDacl,
        SetSecurityDescriptorGroup, SetSecurityDescriptorOwner, TokenImpersonation, TokenUser,
        GENERIC_MAPPING, SECURITY_DESCRIPTOR, TOKEN_IMPERSONATE, TOKEN_USER,
    };
    use windows_sys::Win32::System::Threading::{PROCESS_ALL_ACCESS, PROCESS_DUP_HANDLE};
    let caller = native::process_token(
        unsafe { GetCurrentProcess() },
        TOKEN_QUERY | TOKEN_DUPLICATE,
    )
    .unwrap();
    if native::token_is_system(caller.0) {
        return;
    }
    // Reproduce an administrator-only default DACL even on a normally split
    // user account. Modify a disposable copy, never the caller's real token.
    let mut source = null_mut();
    assert_ne!(
        unsafe {
            DuplicateTokenEx(
                caller.0,
                TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ADJUST_DEFAULT | TOKEN_ASSIGN_PRIMARY,
                null(),
                SecurityImpersonation,
                TokenPrimary,
                &mut source,
            )
        },
        0
    );
    let source = Handle::new(source, "test_source").unwrap();
    let toxic = SecurityDescriptor::from_sddl("D:P(A;;GA;;;SY)(A;;GA;;;BA)").unwrap();
    let mut acl = null_mut();
    let mut present = 0;
    let mut defaulted = 0;
    assert_ne!(
        unsafe {
            GetSecurityDescriptorDacl(
                toxic.attributes().lpSecurityDescriptor,
                &mut present,
                &mut acl,
                &mut defaulted,
            )
        },
        0
    );
    let toxic_dacl = TOKEN_DEFAULT_DACL { DefaultDacl: acl };
    assert_ne!(
        unsafe {
            SetTokenInformation(
                source.0,
                TokenDefaultDacl,
                (&toxic_dacl as *const TOKEN_DEFAULT_DACL).cast(),
                size_of::<TOKEN_DEFAULT_DACL>() as u32,
            )
        },
        0
    );
    let before = default_dacl(source.0);
    let token = restricted(source.0).unwrap();
    assert_eq!(
        default_dacl(source.0),
        before,
        "do not edit the caller's token"
    );
    validate(token.0, native::current_session().unwrap()).unwrap();

    let user = buffer(token.0, TokenUser, "test_user").unwrap();
    let sid = unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let dacl = buffer(token.0, TokenDefaultDacl, "test_default_dacl").unwrap();
    let acl = unsafe { (*dacl.as_ptr().cast::<TOKEN_DEFAULT_DACL>()).DefaultDacl };
    let mut descriptor = SECURITY_DESCRIPTOR::default();
    let descriptor = (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast();
    unsafe {
        assert_ne!(InitializeSecurityDescriptor(descriptor, 1), 0);
        assert_ne!(SetSecurityDescriptorOwner(descriptor, sid, 0), 0);
        assert_ne!(SetSecurityDescriptorGroup(descriptor, sid, 0), 0);
        assert_ne!(SetSecurityDescriptorDacl(descriptor, 1, acl, 0), 0);
    }
    let mut impersonation = null_mut();
    assert_ne!(
        unsafe {
            DuplicateTokenEx(
                token.0,
                TOKEN_QUERY | TOKEN_IMPERSONATE,
                null(),
                SecurityImpersonation,
                TokenImpersonation,
                &mut impersonation,
            )
        },
        0
    );
    let impersonation = Handle::new(impersonation, "test_impersonation").unwrap();
    // The descriptor uses GA, so GenericAll maps it to process rights. Check the
    // effective grant with the actual restricted token, not just ACL text.
    let mut mapping = GENERIC_MAPPING {
        GenericRead: 0,
        GenericWrite: 0,
        GenericExecute: 0,
        GenericAll: PROCESS_ALL_ACCESS,
    };
    let mut privileges = [0usize; 128];
    let mut privileges_size = size_of_val(&privileges) as u32;
    let mut granted = 0;
    let mut allowed = 0;
    assert_ne!(
        unsafe {
            AccessCheck(
                descriptor,
                impersonation.0,
                PROCESS_DUP_HANDLE,
                &mut mapping,
                privileges.as_mut_ptr().cast(),
                &mut privileges_size,
                &mut granted,
                &mut allowed,
            )
        },
        0
    );
    assert_ne!(
        allowed, 0,
        "a restricted child must be able to reopen itself for socket duplication"
    );
    assert_eq!(granted & PROCESS_DUP_HANDLE, PROCESS_DUP_HANDLE);
    let object_security = object_security(token.0).unwrap();
    privileges_size = size_of_val(&privileges) as u32;
    allowed = 0;
    assert_ne!(
        unsafe {
            AccessCheck(
                object_security.attributes().lpSecurityDescriptor,
                impersonation.0,
                PROCESS_DUP_HANDLE,
                &mut mapping,
                privileges.as_mut_ptr().cast(),
                &mut privileges_size,
                &mut granted,
                &mut allowed,
            )
        },
        0
    );
    assert_ne!(
        allowed, 0,
        "explicit process/thread security must grant the ordinary user access too"
    );
}

#[test]
#[ignore = "internal child entry; run through restricted_child_process_can_clone_sockets"]
fn restricted_socket_child_entry() {
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, PROCESS_DUP_HANDLE,
    };
    let token = native::process_token(unsafe { GetCurrentProcess() }, TOKEN_QUERY).unwrap();
    validate(token.0, native::current_session().unwrap()).unwrap();
    let _self_process = Handle::new(
        unsafe { OpenProcess(PROCESS_DUP_HANDLE, 0, GetCurrentProcessId()) },
        "self_process_access",
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (stream, _) = listener.accept().unwrap();
    let mut read = stream.try_clone().unwrap();
    let shutdown = stream.try_clone().unwrap();
    let mut write = stream.try_clone().unwrap();
    let worker = std::thread::Builder::new()
        .name("socket-regression".into())
        .spawn(move || {
            let mut request = [0u8; 4];
            read.read_exact(&mut request).unwrap();
            assert_eq!(&request, b"test");
            write.write_all(b"RFB 003.008\n").unwrap();
        })
        .unwrap();
    peer.set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    peer.write_all(b"test").unwrap();
    let mut banner = [0u8; 12];
    peer.read_exact(&mut banner).unwrap();
    assert_eq!(&banner, b"RFB 003.008\n");
    shutdown.shutdown(Shutdown::Both).unwrap();
    worker.join().unwrap();
    // Distinguish a completed child check from a misspelled libtest filter that
    // executes zero tests and would otherwise also return success.
    std::process::exit(23);
}

#[test]
#[ignore = "run this parent test from an elevated Windows terminal; starts only a temporary test child"]
fn restricted_child_process_can_clone_sockets() {
    use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
    use windows_sys::Win32::System::Threading::{
        CreateProcessAsUserW, GetExitCodeProcess, TerminateProcess, WaitForSingleObject,
        CREATE_NO_WINDOW, PROCESS_INFORMATION, STARTUPINFOW,
    };
    let source = native::process_token(
        unsafe { GetCurrentProcess() },
        TOKEN_QUERY | TOKEN_DUPLICATE,
    )
    .unwrap();
    assert!(!native::token_is_system(source.0));
    assert!(
        native::token_is_elevated(source.0),
        "run from an elevated Windows terminal"
    );
    let token = restricted(source.0).unwrap();
    let security = object_security(token.0).unwrap();
    let attributes = security.attributes();
    let exe = std::env::current_exe().unwrap();
    let mut command = native::wide(format!("\"{}\" --exact windows_service::user_token::tests::restricted_socket_child_entry --ignored --test-threads=1", exe.display()));
    let mut startup = STARTUPINFOW {
        cb: size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut desktop = native::wide("WinSta0\\Default");
    if native::current_session().unwrap() != 0 {
        startup.lpDesktop = desktop.as_mut_ptr();
    }
    let mut process = PROCESS_INFORMATION::default();
    assert_ne!(
        unsafe {
            CreateProcessAsUserW(
                token.0,
                native::wide(&exe).as_ptr(),
                command.as_mut_ptr(),
                &attributes,
                &attributes,
                0,
                CREATE_NO_WINDOW,
                null(),
                null(),
                &startup,
                &mut process,
            )
        },
        0,
        "create restricted test child: {}",
        native::last_error()
    );
    let child = Handle::new(process.hProcess, "test_child").unwrap();
    let _thread = Handle::new(process.hThread, "test_thread").unwrap();
    let wait = unsafe { WaitForSingleObject(child.0, 20000) };
    if wait != WAIT_OBJECT_0 {
        unsafe {
            TerminateProcess(child.0, 1);
            WaitForSingleObject(child.0, 5000);
        }
    }
    assert_eq!(
        wait, WAIT_OBJECT_0,
        "restricted child must finish its socket check"
    );
    let mut code = 0;
    assert_ne!(unsafe { GetExitCodeProcess(child.0, &mut code) }, 0);
    assert_eq!(
        code, 23,
        "the restricted child must complete cloning and bidirectional I/O"
    );
}

#[test]
fn a_restricted_user_token_keeps_identity_and_session_without_admin_or_high_integrity() {
    let source = native::process_token(
        unsafe { GetCurrentProcess() },
        TOKEN_QUERY | TOKEN_DUPLICATE,
    )
    .unwrap();
    let session = native::current_session().unwrap();
    if native::token_is_system(source.0) {
        assert!(
            for_session(source, session).is_err(),
            "SYSTEM must never become the file Agent"
        );
        return;
    }
    let original_sid = user_sid(source.0).unwrap();
    let token = restricted(source.0).unwrap();
    assert_eq!(user_sid(token.0).unwrap(), original_sid);
    validate(token.0, session).unwrap();
    assert!(validate(token.0, session.wrapping_add(1)).is_err());
}

#[test]
fn invalid_token_queries_never_look_like_an_ordinary_user() {
    assert!(validate(null_mut(), 0).is_err());
}

#[test]
fn background_sharing_retains_token_diagnostics_in_its_own_error_category() {
    assert_eq!(
        background_error("windows_service_user_agent_identity_failed:linked_token:1312".into()),
        "background_share_user_context_required:linked_token:1312"
    );
    assert_eq!(
        background_error("windows_service_access_denied".into()),
        "background_share_access_denied"
    );
}
