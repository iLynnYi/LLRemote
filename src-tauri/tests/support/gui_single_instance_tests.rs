use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};

const CHILD_TEST: &str = "gui_single_instance_tests::secondary_process";
const ID_ENV: &str = "LLREMOTE_GUI_TEST_IDENTIFIER";
const STARTED: &str = "LLREMOTE_GUI_SECONDARY_STARTED";
#[cfg(target_os = "windows")]
const CONTEXT_ENV: &str = "LLREMOTE_GUI_TEST_CONTEXT";

fn build_gui(identifier: &str, security_context: Option<&str>) -> tauri::App<MockRuntime> {
    let mut context = mock_context(noop_assets());
    context.config_mut().identifier = identifier.into();
    #[cfg(target_os = "windows")]
    let builder = match security_context {
        Some("same-user") => {
            mock_builder().plugin(super::gui_single_instance::for_test("S-1-5-21-1000"))
        }
        Some("other-user") => {
            mock_builder().plugin(super::gui_single_instance::for_test("S-1-5-21-1001"))
        }
        None => super::configure_gui_builder(mock_builder()),
        _ => panic!("unknown test security context"),
    };
    #[cfg(not(target_os = "windows"))]
    let builder = {
        let _ = security_context;
        super::configure_gui_builder(mock_builder())
    };
    builder
        .build(context)
        .expect("build GUI with the production single-instance gate")
}

#[cfg(target_os = "windows")]
fn pump_messages() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };
    // Keep window dispatch responsive while a child queues a GUI wake, as the
    // real application's main event loop does.
    unsafe {
        let mut message: MSG = std::mem::zeroed();
        while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

#[cfg(target_os = "macos")]
fn pump_messages() {}

fn launch_gui(identifier: &str, security_context: Option<&str>) -> Output {
    let mut command = Command::new(std::env::current_exe().expect("test executable"));
    command
        .args(["--ignored", "--exact", CHILD_TEST, "--nocapture"])
        .env(ID_ENV, identifier);
    #[cfg(target_os = "windows")]
    if let Some(security_context) = security_context {
        command.env(CONTEXT_ENV, security_context);
    } else {
        command.env_remove(CONTEXT_ENV);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = security_context;
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch a separate GUI process");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        pump_messages();
        if child.try_wait().expect("inspect child").is_some() {
            let output = child.wait_with_output().expect("collect child output");
            assert!(output.status.success(), "{output:?}");
            return output;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().expect("reap timed-out child");
            panic!("GUI startup timed out: {output:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "subprocess helper; launched by the single-instance regression test"]
fn secondary_process() {
    let Ok(identifier) = std::env::var(ID_ENV) else {
        return;
    };
    #[cfg(target_os = "windows")]
    let security_context = std::env::var(CONTEXT_ENV).ok();
    #[cfg(not(target_os = "windows"))]
    let security_context: Option<String> = None;
    let app = build_gui(&identifier, security_context.as_deref());
    // A duplicate must exit inside the production gate before reaching here.
    println!("{STARTED}");
    app.cleanup_before_exit();
}

#[test]
fn duplicate_gui_exits_and_can_restart_after_primary_cleanup() {
    // Never contend with the user's running application or touch its stores.
    let identifier = format!("com.llremote.test.{}", uuid::Uuid::new_v4().simple());
    let app = build_gui(&identifier, None);

    for _ in 0..3 {
        let duplicate = launch_gui(&identifier, None);
        assert!(
            !String::from_utf8_lossy(&duplicate.stdout).contains(STARTED),
            "a duplicate GUI reached application initialization: {duplicate:?}"
        );
    }

    app.cleanup_before_exit();
    let restarted = launch_gui(&identifier, None);
    assert!(
        String::from_utf8_lossy(&restarted.stdout).contains(STARTED),
        "GUI could not restart after the primary exited: {restarted:?}"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn same_user_gui_reuses_primary_and_keeps_other_users_independent() {
    // Injected namespace identities cover instance isolation. UAC tokens and
    // Windows login sessions are outside this test's scope.
    let identifier = format!("com.llremote.test.{}", uuid::Uuid::new_v4().simple());
    let primary = build_gui(&identifier, Some("same-user"));
    for _ in 0..3 {
        let duplicate = launch_gui(&identifier, Some("same-user"));
        assert!(
            !String::from_utf8_lossy(&duplicate.stdout).contains(STARTED),
            "{duplicate:?}"
        );
    }
    let other_user = launch_gui(&identifier, Some("other-user"));
    assert!(
        String::from_utf8_lossy(&other_user.stdout).contains(STARTED),
        "{other_user:?}"
    );

    primary.cleanup_before_exit();
    let restarted = launch_gui(&identifier, Some("same-user"));
    assert!(
        String::from_utf8_lossy(&restarted.stdout).contains(STARTED),
        "{restarted:?}"
    );
}
