use super::*;
use std::os::windows::io::AsRawHandle;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, TerminateProcess, PROCESS_TERMINATE,
};

fn system_program(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("SystemRoot").expect("Windows directory"))
        .join("System32")
        .join(name)
}

fn current_parent() -> Handle {
    Handle::new(
        unsafe {
            OpenProcess(
                PROCESS_CREATE_PROCESS | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                GetCurrentProcessId(),
            )
        },
        "test_parent",
    )
    .unwrap()
}

fn current_launch_mode() -> LaunchMode {
    let token = native::process_token(unsafe { GetCurrentProcess() }, TOKEN_QUERY).unwrap();
    if elevated(token.0).unwrap() {
        LaunchMode::RestrictedToken
    } else {
        LaunchMode::InheritDesktop
    }
}

#[test]
fn executable_paths_keep_spaces_and_unicode_in_the_command_line() {
    let path = Path::new(r"C:\Users\示例用户\LLRemote Sharing\share.exe");
    let command = command_line(path, "--background-share-idle");
    assert_eq!(
        String::from_utf16(&command[..command.len() - 1]).unwrap(),
        format!("\"{}\" --background-share-idle", path.display())
    );
    assert_eq!(command.last(), Some(&0));
}

#[test]
fn early_process_exit_keeps_its_numeric_exit_code() {
    let process = spawn(
        &current_parent(),
        &system_program("cmd.exe"),
        "/d /c exit 23",
        current_launch_mode(),
    )
    .unwrap();
    assert_eq!(
        unsafe { WaitForSingleObject(process.handle.0, 5000) },
        WAIT_OBJECT_0
    );
    assert_eq!(
        process.check_running().unwrap_err(),
        "background_share_runtime_failed: stage=process_exit;exit_code=23"
    );
}

#[test]
fn creation_failure_keeps_the_stage_and_win32_code_without_the_path() {
    let missing = system_program("llremote-absent-test-binary.exe");
    let error = match spawn(
        &current_parent(),
        &missing,
        "--background-share-idle",
        current_launch_mode(),
    ) {
        Ok(_) => panic!("the missing executable cannot start"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        "background_share_startup_unavailable: stage=create_process;win32=2"
    );
}

#[test]
#[ignore = "run from an elevated terminal in the logged-in user's desktop; no VNC listener is started"]
fn elevated_launcher_creates_a_same_user_non_elevated_background() {
    let token = native::process_token(unsafe { GetCurrentProcess() }, TOKEN_QUERY).unwrap();
    assert!(
        elevated(token.0).unwrap(),
        "this regression needs an elevated launcher"
    );
    struct StopChild(Process);
    impl Drop for StopChild {
        fn drop(&mut self) {
            unsafe {
                TerminateProcess(self.0.handle.0, 0);
                WaitForSingleObject(self.0.handle.0, 5000);
            }
        }
    }
    let child = StopChild(
        launch(
            &system_program(r"WindowsPowerShell\v1.0\powershell.exe"),
            "-NoProfile -NonInteractive -Command Start-Sleep -Seconds 30",
        )
        .unwrap(),
    );
    validate_child(child.0.handle.0).unwrap();
    child.0.check_running().unwrap();
}

#[test]
#[ignore = "run from an elevated interactive user terminal; verifies pre-resume rejection"]
fn elevated_inherited_child_is_rejected_before_its_command_runs() {
    let token = native::process_token(unsafe { GetCurrentProcess() }, TOKEN_QUERY).unwrap();
    assert!(
        elevated(token.0).unwrap(),
        "this regression needs an elevated launcher"
    );
    let marker = std::env::temp_dir().join(format!(
        "llremote-rejected-child-{}.txt",
        uuid::Uuid::new_v4()
    ));
    let argument = format!(
        "-NoProfile -NonInteractive -Command \"[IO.File]::WriteAllText('{}','executed')\"",
        marker.display().to_string().replace('\'', "''")
    );
    let result = spawn(
        &current_parent(),
        &system_program(r"WindowsPowerShell\v1.0\powershell.exe"),
        &argument,
        LaunchMode::InheritDesktop,
    );
    if let Ok(child) = &result {
        unsafe {
            TerminateProcess(child.handle.0, 1);
            WaitForSingleObject(child.handle.0, 5000);
        }
    }
    assert!(
        result.is_err(),
        "an elevated inherited child must not be resumed"
    );
    assert!(!marker.exists(), "rejected child code must never execute");
}

// SYNCHRONIZE is a standard access right, absent from the Threading bindings.
const PROCESS_WAIT_ACCESS: u32 = 0x0010_0000;

const HELPER: &str = "background_share::startup_windows::tests::background_launch_job_helper";
const RESULT_VARIABLE: &str = "VNC_BACKGROUND_LAUNCH_TEST_RESULT";

#[test]
#[ignore = "subprocess helper for the interactive Job lifetime regression"]
fn background_launch_job_helper() {
    let result =
        std::path::PathBuf::from(std::env::var_os(RESULT_VARIABLE).expect("test result path"));
    let deadline = Instant::now() + Duration::from_secs(15);
    while !result.with_extension("ready").exists() {
        assert!(
            Instant::now() < deadline,
            "parent must assign the helper to its Job first"
        );
        sleep(Duration::from_millis(20));
    }
    let attempt = launch(
        &system_program(r"WindowsPowerShell\v1.0\powershell.exe"),
        "-NoProfile -NonInteractive -Command Start-Sleep -Seconds 45",
    );
    match attempt {
        Ok(process) => {
            let pid = unsafe { GetProcessId(process.handle.0) };
            std::fs::write(&result, pid.to_string()).unwrap();
            // The outer test kills this helper by closing its own Job handle.
            sleep(Duration::from_secs(30));
        }
        Err(error) => {
            std::fs::write(&result, error).unwrap();
            panic!("desktop-parent launch failed");
        }
    }
}

struct Cleanup {
    helper: Child,
    background: Option<Handle>,
    directory: std::path::PathBuf,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(process) = &self.background {
            unsafe {
                TerminateProcess(process.0, 0);
                WaitForSingleObject(process.0, 5000);
            }
        }
        let _ = self.helper.kill();
        let _ = self.helper.wait();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[test]
#[ignore = "requires the logged-in user's interactive desktop; no VNC listener is started"]
fn background_host_survives_a_launcher_job_without_breakaway() {
    let directory = std::env::temp_dir().join(format!(
        "vnc-share-launch-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let result = directory.join("result");
    let helper = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", HELPER])
        .env(RESULT_VARIABLE, &result)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut cleanup = Cleanup {
        helper,
        background: None,
        directory,
    };
    let job = Handle::new(unsafe { CreateJobObjectW(null(), null()) }, "test_job").unwrap();
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    assert_ne!(
        unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        },
        0
    );
    assert_ne!(
        unsafe { AssignProcessToJobObject(job.0, cleanup.helper.as_raw_handle()) },
        0
    );
    std::fs::write(result.with_extension("ready"), "ready").unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !result.exists() {
        assert!(
            Instant::now() < deadline,
            "helper must report its launch result"
        );
        assert!(
            cleanup.helper.try_wait().unwrap().is_none(),
            "helper exited before reporting"
        );
        sleep(Duration::from_millis(20));
    }
    let reported = std::fs::read_to_string(&result).unwrap();
    let pid: u32 = reported.parse().unwrap_or_else(|_| panic!("{reported}"));
    let background = Handle::new(
        unsafe {
            OpenProcess(
                PROCESS_WAIT_ACCESS | PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                pid,
            )
        },
        "test_background",
    )
    .unwrap();
    cleanup.background = Some(background);
    drop(job);
    assert_eq!(
        unsafe { WaitForSingleObject(cleanup.helper.as_raw_handle(), 5000) },
        WAIT_OBJECT_0,
        "closing the restrictive Job must kill its helper"
    );
    assert_eq!(
        unsafe { WaitForSingleObject(cleanup.background.as_ref().unwrap().0, 1000) },
        WAIT_TIMEOUT,
        "desktop-parent host must survive the launcher's Job closing"
    );
}
