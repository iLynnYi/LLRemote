//! Run personal cleanup in an existing owner session via Task Scheduler. An
//! administrator cannot assume another user's process-token DACL allows copy.
use super::identity::valid_user_sid;

#[derive(Clone, Copy)]
pub(super) enum UserAction {
    Check,
    Stop,
    Clean { delete_data: bool },
}

fn definition(
    sid: &str,
    executable: &str,
    directory: &str,
    action: UserAction,
    result: Option<&str>,
) -> Result<String, String> {
    if !valid_user_sid(sid) {
        return Err("windows_uninstall_owner_mismatch".into());
    }
    let escape = |value: &str| {
        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    let command = escape(executable);
    let directory = escape(directory);
    let (verb, purge) = match action {
        UserAction::Check => ("--uninstall-user-check", ""),
        UserAction::Stop => ("--uninstall-user-stop", ""),
        UserAction::Clean { delete_data } => (
            "--uninstall-user-clean",
            if delete_data { " --delete-data" } else { "" },
        ),
    };
    let result = result
        .map(|path| format!(" --uninstall-result &quot;{}&quot;", escape(path)))
        .unwrap_or_default();
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Principals><Principal id="Owner"><UserId>{sid}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
  <Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><AllowHardTerminate>true</AllowHardTerminate><StartWhenAvailable>false</StartWhenAvailable><AllowStartOnDemand>true</AllowStartOnDemand><Enabled>true</Enabled><Hidden>true</Hidden><ExecutionTimeLimit>PT90S</ExecutionTimeLimit></Settings>
  <Actions Context="Owner"><Exec><Command>{command}</Command><Arguments>{verb} --target-user {sid}{purge} --uninstall-directory &quot;{directory}&quot;{result}</Arguments></Exec></Actions>
</Task>"#
    ))
}

#[cfg(target_os = "windows")]
pub(super) fn run(
    user: &super::user::User,
    directory: &std::path::Path,
    action: UserAction,
) -> Result<super::report::CleanupReport, String> {
    platform::run(user, directory, action)
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{definition, UserAction};
    use crate::windows_service::native;
    use std::time::{Duration, Instant};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::TaskScheduler::{
        ITaskFolder, ITaskService, TaskScheduler, TASK_CREATE, TASK_DONT_ADD_PRINCIPAL_ACE,
        TASK_LOGON_INTERACTIVE_TOKEN, TASK_STATE_QUEUED, TASK_STATE_RUNNING,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows_core::BSTR;

    pub(super) fn run(
        user: &super::super::user::User,
        directory: &std::path::Path,
        action: UserAction,
    ) -> Result<super::super::report::CleanupReport, String> {
        use super::super::report::CleanupReport;
        let sid = user.sid.as_str();
        let output = matches!(action, UserAction::Stop | UserAction::Clean { .. }).then(|| {
            TemporaryReport(
                user.local
                    .join("Temp")
                    .join(format!("LLRemote-cleanup-{}.json", uuid::Uuid::new_v4())),
            )
        });

        if !native::current_elevation()? {
            return Err("windows_uninstall_admin_required".into());
        }
        let executable = std::env::current_exe().map_err(|_| "windows_uninstall_binary_missing")?;
        if executable.parent() != Some(directory) {
            // The bundled helper is in the elevated uninstaller's temp directory.
            // The verified owner session needs read/execute access to this file.
            native::protect_file(&executable, true)
                .map_err(|error| format!("windows_uninstall_user_task_failed:{error}"))?;
        }
        let xml = definition(
            sid,
            &executable.to_string_lossy(),
            &directory.to_string_lossy(),
            action,
            output
                .as_ref()
                .map(|path| path.0.to_string_lossy())
                .as_deref(),
        )?;
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }.map_err(failure)?;
        let _apartment = Apartment;
        let service: ITaskService =
            unsafe { CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER) }
                .map_err(failure)?;
        let empty = VARIANT::default();
        unsafe { service.Connect(&empty, &empty, &empty, &empty) }.map_err(failure)?;
        let folder = unsafe { service.GetFolder(&BSTR::from("\\")) }.map_err(failure)?;
        let name = BSTR::from(format!("LLRemote-Uninstall-{}", uuid::Uuid::new_v4()));
        let task = unsafe {
            folder.RegisterTask(
                &name,
                &BSTR::from(xml),
                TASK_CREATE.0 | TASK_DONT_ADD_PRINCIPAL_ACE.0,
                &VARIANT::from(sid),
                &empty,
                TASK_LOGON_INTERACTIVE_TOKEN,
                &VARIANT::from("D:P(A;;FA;;;SY)(A;;FA;;;BA)"),
            )
        }
        .map_err(failure)?;
        let registration = Registration { folder, name };
        let _running = unsafe { task.Run(&empty) }.map_err(failure)?;
        let deadline = Instant::now() + Duration::from_secs(90);
        let result = loop {
            let state = unsafe { task.State() }.map_err(failure)?;
            let exit = unsafe { task.LastTaskResult() }.map_err(failure)?;
            if state != TASK_STATE_QUEUED
                && state != TASK_STATE_RUNNING
                && exit != 0x41303
                && exit != 0x41301
            {
                break exit;
            }
            if Instant::now() >= deadline {
                unsafe { task.Stop(0) }.map_err(failure)?;
                return Err("windows_uninstall_user_task_timeout".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let task_residue = registration.remove().err();
        if !CleanupReport::completed(result) {
            return Err(super::super::errors::from_exit(result as u32));
        }
        let mut report = if let Some(path) = &output {
            CleanupReport::read(&path.0).unwrap_or_else(|_| {
                let mut report = CleanupReport::default();
                report.warning(
                    "windows_uninstall_report_failed",
                    user.local.join("LLRemote"),
                );
                report
            })
        } else {
            CleanupReport::default()
        };
        if let Some(error) = task_residue {
            report.warning(&error, "Windows Task Scheduler");
        }
        report.reboot |= matches!(result, 201 | 202);
        if matches!(result, 200 | 202) && report.warnings.is_empty() {
            report.warning(
                "windows_uninstall_report_failed",
                user.local.join("LLRemote"),
            );
        }
        Ok(report)
    }

    struct TemporaryReport(std::path::PathBuf);
    impl Drop for TemporaryReport {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn failure(error: windows_core::Error) -> String {
        format!("windows_uninstall_user_task_failed:{:08x}", error.code().0)
    }

    struct Registration {
        folder: ITaskFolder,
        name: BSTR,
    }
    impl Registration {
        fn remove(self) -> Result<(), String> {
            unsafe { self.folder.DeleteTask(&self.name, 0) }.map_err(failure)
        }
    }
    impl Drop for Registration {
        fn drop(&mut self) {
            if let Ok(task) = unsafe { self.folder.GetTask(&self.name) } {
                let _ = unsafe { task.Stop(0) };
            }
            let _ = unsafe { self.folder.DeleteTask(&self.name, 0) };
        }
    }
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(test)]
#[path = "../../tests/support/windows_uninstall_user_task_tests.rs"]
mod tests;
