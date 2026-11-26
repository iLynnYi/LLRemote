use super::{
    config,
    ipc::{self, Request},
    native::{self, Handle},
    password_management, repair, supervisor, uac, worker, RepairKind, RepairStatus,
    WindowsServiceConfigureRequest, WindowsServiceOverview, WindowsServicePasswordRequest,
    WindowsServiceRepairReport,
};
use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{ERROR_SERVICE_DOES_NOT_EXIST, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT, READ_CONTROL, WRITE_DAC,
};
use windows_sys::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows_sys::Win32::System::Services::*;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, GetExitCodeProcess, GetProcessId, WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS,
    SHELLEXECUTEINFOW,
};

struct ScHandle(SC_HANDLE);
impl Drop for ScHandle {
    fn drop(&mut self) {
        unsafe {
            CloseServiceHandle(self.0);
        }
    }
}

fn scm(access: u32) -> Result<ScHandle, String> {
    let handle = unsafe { OpenSCManagerW(null(), null(), access) };
    if handle.is_null() {
        Err("windows_service_admin_required".into())
    } else {
        Ok(ScHandle(handle))
    }
}

fn open_service(access: u32) -> Result<Option<ScHandle>, String> {
    let scm = scm(SC_MANAGER_CONNECT)?;
    let handle =
        unsafe { OpenServiceW(scm.0, native::wide(native::SERVICE_NAME).as_ptr(), access) };
    if !handle.is_null() {
        return Ok(Some(ScHandle(handle)));
    }
    if native::last_error() == ERROR_SERVICE_DOES_NOT_EXIST {
        Ok(None)
    } else {
        Err("windows_service_access_denied".into())
    }
}

fn query(service: &ScHandle) -> Result<SERVICE_STATUS_PROCESS, String> {
    let mut status = SERVICE_STATUS_PROCESS::default();
    let mut needed = 0;
    if unsafe {
        QueryServiceStatusEx(
            service.0,
            SC_STATUS_PROCESS_INFO,
            (&mut status as *mut SERVICE_STATUS_PROCESS).cast(),
            size_of::<SERVICE_STATUS_PROCESS>() as u32,
            &mut needed,
        )
    } == 0
    {
        Err("windows_service_internal".into())
    } else {
        Ok(status)
    }
}

pub(super) fn service_process_id() -> Result<Option<u32>, String> {
    let Some(service) = open_service(SERVICE_QUERY_STATUS)? else {
        return Ok(None);
    };
    let process = query(&service)?.dwProcessId;
    Ok((process != 0).then_some(process))
}

// Read startup policy from SCM rather than infer it from saved credentials.
pub(super) fn service_auto_start() -> Option<bool> {
    let service = open_service(SERVICE_QUERY_CONFIG).ok()??;
    let mut needed = 0;
    unsafe {
        QueryServiceConfigW(service.0, null_mut(), 0, &mut needed);
    }
    if needed == 0 || needed > 64 * 1024 {
        return None;
    }
    let mut storage = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
    let config = storage.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>();
    if unsafe { QueryServiceConfigW(service.0, config, needed, &mut needed) } == 0 {
        return None;
    }
    Some(unsafe { (*config).dwStartType } == SERVICE_AUTO_START)
}

pub(super) fn failure_diagnostics() -> serde_json::Value {
    let scm = open_service(SERVICE_QUERY_STATUS)
        .and_then(|service| service.ok_or_else(|| "windows_service_not_installed".to_string()))
        .and_then(|service| query(&service));
    let last_attempt = native::data_directory()
        .ok()
        .and_then(|directory| crate::startup_attempt::read(&directory));
    match scm {
        Ok(status) => serde_json::json!({
            "scm_state": status.dwCurrentState, "process_id": status.dwProcessId,
            "win32_exit_code": status.dwWin32ExitCode,
            "service_exit_code": status.dwServiceSpecificExitCode,
            "checkpoint": status.dwCheckPoint, "last_service_attempt": last_attempt,
        }),
        Err(error) => {
            serde_json::json!({"status_error": error, "last_service_attempt": last_attempt})
        }
    }
}

fn set_auto_start_direct(enabled: bool) -> Result<(), String> {
    let _lifecycle_gate = native::lifecycle_lock()?;
    super::auto_start::set_enabled(enabled, &mut NativeAutoStart)
}

struct NativeAutoStart;

impl super::auto_start::AutoStartOperations for NativeAutoStart {
    fn is_running(&mut self) -> Result<bool, String> {
        let service = open_service(SERVICE_QUERY_STATUS)?
            .ok_or_else(|| "windows_service_not_installed".to_string())?;
        match query(&service)?.dwCurrentState {
            SERVICE_RUNNING => Ok(true),
            SERVICE_STOPPED => Ok(false),
            SERVICE_START_PENDING => {
                wait_service(&service, SERVICE_RUNNING)?;
                Ok(true)
            }
            SERVICE_STOP_PENDING => {
                wait_service(&service, SERVICE_STOPPED)?;
                Ok(false)
            }
            _ => Err("windows_service_internal".into()),
        }
    }

    fn start_configuration_only(&mut self) -> Result<(), String> {
        start_direct(true)
    }

    fn readiness(&mut self) -> Result<super::auto_start::AutoStartReadiness, String> {
        // Direct IPC is required: stopped status has only credential reference metadata.
        let overview = ipc::call(&Request::Status)?
            .into_result()?
            .overview
            .ok_or_else(|| "windows_service_ipc_unavailable".to_string())?;
        Ok(super::auto_start::AutoStartReadiness::new(
            overview.bind_address.as_deref(),
            overview.port,
            overview.credential_configured,
        ))
    }

    fn stop(&mut self) -> Result<(), String> {
        stop_direct()
    }

    fn set_enabled(&mut self, enabled: bool) -> Result<(), String> {
        let service = open_service(SERVICE_CHANGE_CONFIG)?
            .ok_or_else(|| "windows_service_not_installed".to_string())?;
        if unsafe {
            ChangeServiceConfigW(
                service.0,
                SERVICE_NO_CHANGE,
                if enabled {
                    SERVICE_AUTO_START
                } else {
                    SERVICE_DEMAND_START
                },
                SERVICE_NO_CHANGE,
                null(),
                null(),
                null_mut(),
                null(),
                null(),
                null(),
                null(),
            )
        } == 0
        {
            return Err("windows_service_configure_failed".into());
        }
        Ok(())
    }
}

pub(super) fn status() -> Result<WindowsServiceOverview, String> {
    let mut overview = WindowsServiceOverview::unsupported();
    overview.supported = true;
    overview.sas_policy = super::sas::policy();
    overview.status = "not_installed".into();
    let Some(service) = open_service(SERVICE_QUERY_STATUS)? else {
        return Ok(overview);
    };
    overview.installed = true;
    overview.last_service_attempt = native::data_directory()
        .ok()
        .and_then(|directory| crate::startup_attempt::read(&directory));
    match password_management::current_user_authorized() {
        Ok(authorized) => overview.password_management_authorized = authorized,
        Err(code) => overview.last_error_code = Some(code),
    }
    overview.auto_start = service_auto_start();
    let current = query(&service)?;
    overview.status = match current.dwCurrentState {
        SERVICE_RUNNING => "running",
        SERVICE_STOPPED => "stopped",
        SERVICE_START_PENDING => "start_pending",
        SERVICE_STOP_PENDING => "stop_pending",
        _ => "unknown",
    }
    .into();
    if current.dwCurrentState == SERVICE_RUNNING {
        match ipc::call(&Request::Status).and_then(ipc::Response::into_result) {
            Ok(response) => {
                if let Some(mut state) = response.overview {
                    state.auto_start = overview.auto_start;
                    if overview.last_service_attempt.is_some() {
                        state.last_service_attempt = overview.last_service_attempt;
                    }
                    state.password_management_authorized = overview.password_management_authorized;
                    if overview.last_error_code.is_some() {
                        state.last_error_code = overview.last_error_code;
                    }
                    return Ok(state);
                }
            }
            Err(code) => overview.last_error_code = Some(code),
        }
    }
    if let Ok(Some(saved)) = config::read() {
        saved.apply(&mut overview);
        // Reference metadata only; the LocalSystem service checks the actual
        // credential before opening its listener on the next start.
        overview.credential_configured = true;
    }
    Ok(overview)
}

fn source_binary() -> Result<std::path::PathBuf, String> {
    let current =
        std::env::current_exe().map_err(|_| "windows_service_binary_missing".to_string())?;
    // Both the App and dedicated helper dispatch maintenance before creating UI.
    // Always copy the caller's current version, never a stale sibling sidecar.
    let source = current;
    if !source.is_file() {
        return Err("windows_service_binary_missing".into());
    }
    native::reject_reparse(&source)?;
    Ok(source)
}

fn launch_helper(binary: &std::path::Path, arguments: &str) -> Result<Handle, String> {
    native::reject_reparse(binary)?;
    let binary = binary.to_owned();
    let arguments = arguments.to_owned();
    // Commands run on the async runtime's blocking pool, without a Shell message
    // loop or a known COM apartment. Give runas a fresh STA and finish Shell
    // activation before that thread exits (SEE_MASK_NOASYNC).
    let handle = std::thread::Builder::new()
        .name("windows-service-uac".into())
        .spawn(move || {
            if unsafe {
                CoInitializeEx(
                    null(),
                    (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
                )
            } < 0
            {
                return Err("windows_service_elevation_failed".to_string());
            }
            struct Apartment;
            impl Drop for Apartment {
                fn drop(&mut self) {
                    unsafe { CoUninitialize() };
                }
            }
            let _apartment = Apartment;
            launch_helper_sta(&binary, &arguments)
        })
        .map_err(|_| "windows_service_elevation_failed".to_string())?
        .join()
        .map_err(|_| "windows_service_elevation_failed".to_string())??;
    Handle::new(handle as _, "windows_service_elevation_failed")
}

fn create_elevation_relay() -> Result<(String, Handle), String> {
    uac::check_elevation()?;
    let id = uuid::Uuid::new_v4().to_string();
    let pipe = ipc::create_server(
        &ipc::relay_name(&id)?,
        "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;OW)",
    )?;
    Ok((id, pipe))
}

fn launch_helper_sta(binary: &std::path::Path, arguments: &str) -> Result<isize, String> {
    let file = native::wide(binary);
    let verb = native::wide("runas");
    let parameters = native::wide(arguments);
    let directory = native::wide(
        binary
            .parent()
            .ok_or_else(|| "windows_service_binary_missing".to_string())?,
    );
    let mut execute = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: parameters.as_ptr(),
        lpDirectory: directory.as_ptr(),
        nShow: 0,
        ..Default::default()
    };
    if unsafe { ShellExecuteExW(&mut execute) } == 0 {
        return Err(super::maintenance::elevation_error(native::last_error()).into());
    }
    // Transfer ownership of the process handle to the caller's wait/RAII guard.
    if execute.hProcess.is_null() {
        return Err("windows_service_elevation_failed".into());
    }
    Ok(execute.hProcess as isize)
}

fn wait_helper(process: Handle, category: &str, action: Option<&str>) -> Result<(), String> {
    match unsafe { WaitForSingleObject(process.0, 60_000) } {
        WAIT_OBJECT_0 => helper_result(&process, category, action),
        WAIT_TIMEOUT => Err("windows_service_ipc_timeout".into()),
        _ => Err(category.into()),
    }
}

fn require_live_helper(process: &Handle, category: &str, action: &str) -> Result<(), String> {
    match unsafe { WaitForSingleObject(process.0, 0) } {
        WAIT_TIMEOUT => Ok(()),
        WAIT_OBJECT_0 => helper_result(process, category, Some(action)).and(Err(category.into())),
        _ => Err(category.into()),
    }
}

fn helper_result(process: &Handle, category: &str, action: Option<&str>) -> Result<(), String> {
    let mut exit = 1;
    if unsafe { GetExitCodeProcess(process.0, &mut exit) } == 0 {
        return Err(category.into());
    }
    if exit == 0 {
        return Ok(());
    }
    let error = super::maintenance::helper_error(action, exit).unwrap_or(category);
    if matches!(action, Some("--start" | "--stop")) {
        Err(format!("{error}:stage=elevated_helper;exit_code={exit}"))
    } else {
        Err(error.into())
    }
}

pub(super) fn perform_action(verb: &str) -> Result<WindowsServiceOverview, String> {
    let lifecycle = matches!(verb, "--start" | "--stop");
    if lifecycle {
        match lifecycle_action(verb) {
            Ok(()) => return status(),
            Err(code) if code == "windows_service_access_denied" && !native::is_elevated() => {}
            Err(code) => return Err(code),
        }
    }
    if native::is_elevated() {
        direct_action(verb)?;
    } else {
        if verb == "--install" {
            uac::check_elevation()?;
        }
        // Use this App's helper for lifecycle authorization and startup gates;
        // an older installed helper must not bypass the saved-credential check.
        let binary = if verb == "--install"
            || lifecycle
            || verb == "--authorize-password-management"
            || matches!(verb, "--enable-autostart" | "--disable-autostart")
        {
            source_binary()?
        } else {
            native::installed_binary()?
        };
        if !binary.is_file() {
            return Err("windows_service_binary_missing".into());
        }
        let arguments =
            if verb == "--install" || lifecycle || verb == "--authorize-password-management" {
                format!("{verb} --operator-pid {}", unsafe { GetCurrentProcessId() })
            } else {
                verb.to_string()
            };
        let helper = launch_helper(&binary, &arguments)?;
        let code = match verb {
            "--install" => "windows_service_install_failed",
            "--uninstall" => "windows_service_uninstall_failed",
            "--start" => "windows_service_start_failed",
            "--enable-sas" => "windows_service_sas_policy_configure_failed",
            "--enable-autostart" | "--disable-autostart" => "windows_service_configure_failed",
            "--authorize-password-management" => {
                "windows_service_password_management_authorization_failed"
            }
            _ => "windows_service_stop_failed",
        };
        wait_helper(helper, code, Some(verb))?;
    }
    status()
}

pub(super) fn repair_sharing() -> Result<WindowsServiceRepairReport, String> {
    let initial = status()?;
    let elevated = native::is_elevated();
    let mut authorization = uac::inspect_or_repair(elevated);
    if !initial.installed {
        let mut report = crate::background_share::repair_sharing().unwrap_or_else(|code| {
            WindowsServiceRepairReport {
                overview: initial,
                items: vec![repair::item(
                    RepairKind::Background,
                    RepairStatus::Failed,
                    Some(code),
                )],
            }
        });
        report.items.push(authorization);
        return Ok(report);
    }
    let update_check = repair_update_required();
    let mut report = inspect_service_repair(initial, &update_check);
    if !report
        .items
        .iter()
        .any(|item| item.status == RepairStatus::NeedsAdministrator)
    {
        report.items.push(authorization);
        return Ok(report);
    }
    let result = if elevated {
        native::current_user_sid().and_then(|user_sid| run_service_repair(user_sid, &update_check))
    } else {
        request_service_repair()
    };
    match result {
        Ok(repaired) => {
            report = repaired;
            if !elevated {
                authorization = repair::item(RepairKind::Elevation, RepairStatus::Healthy, None);
            }
        }
        Err(code) => {
            let authorization_error = matches!(
                code.as_str(),
                repair::UAC_DISABLED
                    | "windows_service_uac_cancelled"
                    | "windows_service_elevation_denied"
                    | "windows_service_elevation_failed"
                    | "windows_service_admin_required"
            );
            if !authorization_error {
                // The helper might have applied some changes before reporting failed.
                if let Ok(view) = status() {
                    if view.installed {
                        report = inspect_service_repair(view, &repair_update_required());
                    }
                }
            }
            if elevated {
                // Keep the UAC result even when the service's final state could not be verified.
                if let Some(service) = report
                    .items
                    .iter_mut()
                    .find(|item| item.kind == RepairKind::Service)
                {
                    *service = repair::item(RepairKind::Service, RepairStatus::Unknown, Some(code));
                }
            } else {
                authorization = repair::item(
                    RepairKind::Elevation,
                    if authorization_error {
                        RepairStatus::NeedsAdministrator
                    } else {
                        RepairStatus::Unknown
                    },
                    Some(code),
                );
            }
        }
    }
    report.items.push(authorization);
    Ok(report)
}

fn inspect_service_repair(
    view: WindowsServiceOverview,
    update: &Result<bool, String>,
) -> WindowsServiceRepairReport {
    let mut report = repair::inspect(view, matches!(update, Ok(true)));
    record_update_check(&mut report, update);
    report
}

fn run_service_repair(
    user_sid: String,
    update: &Result<bool, String>,
) -> Result<WindowsServiceRepairReport, String> {
    let mut report = repair::run(&mut NativeRepair {
        user_sid,
        update_required: matches!(update, Ok(true)),
    })?;
    record_update_check(&mut report, update);
    Ok(report)
}

fn record_update_check(report: &mut WindowsServiceRepairReport, update: &Result<bool, String>) {
    if let Err(code) = update {
        if let Some(service) = report
            .items
            .iter_mut()
            .find(|item| item.kind == RepairKind::Service && item.status == RepairStatus::Healthy)
        {
            *service = repair::item(
                RepairKind::Service,
                RepairStatus::Unknown,
                Some(code.clone()),
            );
        }
    }
}

fn request_service_repair() -> Result<WindowsServiceRepairReport, String> {
    const ACTION: &str = "--repair-sharing-relay";
    const ERROR: &str = "windows_service_configure_failed";
    let (id, pipe) = create_elevation_relay()?;
    let arguments = format!("{ACTION} {id} --parent-pid {}", unsafe {
        GetCurrentProcessId()
    });
    let helper = launch_helper(&source_binary()?, &arguments)?;
    let report = ipc::receive_repair_report(pipe.0, unsafe { GetProcessId(helper.0) }, || {
        require_live_helper(&helper, ERROR, ACTION)
    })?;
    wait_helper(helper, ERROR, Some(ACTION))?;
    Ok(report)
}

struct NativeRepair {
    user_sid: String,
    update_required: bool,
}

// Compare only on explicit repair, never during periodic status polling. An
// old stopped service can be detected without opening a public listener.
fn repair_update_required() -> Result<bool, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    fn digest(path: &std::path::Path) -> Result<[u8; 32], String> {
        let mut file =
            std::fs::File::open(path).map_err(|_| "windows_service_binary_missing".to_string())?;
        let mut hash = Sha256::new();
        let mut bytes = [0u8; 64 * 1024];
        loop {
            let count = file
                .read(&mut bytes)
                .map_err(|_| "windows_service_access_denied".to_string())?;
            if count == 0 {
                break;
            }
            hash.update(&bytes[..count]);
        }
        Ok(hash.finalize().into())
    }
    let source = source_binary()?;
    let installed = native::installed_binary()?;
    if source == installed {
        return Ok(false);
    }
    if !installed.is_file() {
        return Ok(true);
    }
    Ok(digest(&source)? != digest(&installed)?)
}

impl super::repair::RepairOperations for NativeRepair {
    fn update_required(&self) -> bool {
        self.update_required
    }
    fn snapshot(&mut self) -> Result<WindowsServiceOverview, String> {
        let mut view = status()?;
        // Alternate UAC credentials must authorize the account that clicked
        // Repair, rather than granting the elevated helper's own account.
        view.password_management_authorized =
            password_management::authorized(false, &self.user_sid)?;
        Ok(view)
    }
    fn update_service(&mut self) -> Result<(), String> {
        let was_running = status()?.status == "running";
        install_direct(&self.user_sid)?;
        self.update_required = repair_update_required()?;
        if self.update_required {
            return Err("windows_service_install_failed".into());
        }
        if !was_running {
            return Ok(());
        }
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let view = status()?;
            if view.worker_running {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(view
                    .last_error_code
                    .unwrap_or("windows_service_worker_start_failed".into()));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    fn authorize(&mut self) -> Result<(), String> {
        authorize_password_management_direct(&self.user_sid)
    }
    fn repair_sas(&mut self) -> Result<(), String> {
        super::sas::repair_service_policy()
    }
    fn repair_agent(&mut self) -> Result<(), String> {
        repair_user_agent_direct()
    }
}

fn repair_user_agent_direct() -> Result<(), String> {
    let overview = status()?;
    if !overview.installed {
        return Err("windows_service_not_installed".into());
    }
    if !overview.user_agent_repair_supported {
        return Err("windows_service_protocol_incompatible".into());
    }
    ipc::call(&Request::RepairUserAgent)?.into_result()?;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let overview = ipc::call(&Request::Status)?
            .into_result()?
            .overview
            .ok_or("windows_service_ipc_unavailable")?;
        if super::repair::agent_ready(&overview)? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("windows_service_user_agent_repair_timeout".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub(super) fn install(
    request: Option<WindowsServiceConfigureRequest>,
) -> Result<WindowsServiceOverview, String> {
    let Some(request) = request else {
        let overview = status()?;
        if !overview.credential_configured
            || overview.bind_address.is_none()
            || overview.port.is_none()
        {
            return Err("windows_service_credential_missing".into());
        }
        return perform_action("--install");
    };
    let overview = status()?;
    super::setup::validate(
        &request,
        service_configured(&overview),
        overview.credential_configured,
    )?;
    if native::is_elevated() {
        setup_direct(request, &native::current_user_sid()?)?;
    } else {
        const ACTION: &str = "--install-relay";
        const ERROR: &str = "windows_service_install_failed";
        let binary = source_binary()?;
        let (id, pipe) = create_elevation_relay()?;
        let arguments = format!("{ACTION} {id} --parent-pid {}", unsafe {
            GetCurrentProcessId()
        });
        let helper = launch_helper(&binary, &arguments)?;
        let pid = unsafe { GetProcessId(helper.0) };
        ipc::relay_to_helper(pipe.0, pid, &request, || {
            require_live_helper(&helper, ERROR, ACTION)
        })?;
        wait_helper(helper, ERROR, Some(ACTION))?;
    }
    status()
}

fn service_configured(overview: &WindowsServiceOverview) -> bool {
    overview.credential_configured && overview.bind_address.is_some() && overview.port.is_some()
}

struct NativeSetup<'a> {
    user_sid: &'a str,
}
impl super::setup::Operations for NativeSetup<'_> {
    fn inspect(&mut self) -> Result<(bool, bool), String> {
        let overview = status()?;
        if overview.installed && overview.status != "stopped" {
            return Err("windows_service_invalid_config".into());
        }
        Ok((
            service_configured(&overview),
            overview.credential_configured,
        ))
    }
    fn install(&mut self) -> Result<(), String> {
        install_locked(self.user_sid)
    }
    fn configure(&mut self, request: WindowsServiceConfigureRequest) -> Result<(), String> {
        configure_locked(request)
    }
    fn authorize(&mut self) -> Result<(), String> {
        password_management::grant(self.user_sid)
            .map_err(|_| "windows_service_install_authorization_failed".into())
    }
}

fn setup_direct(request: WindowsServiceConfigureRequest, user_sid: &str) -> Result<(), String> {
    if !native::is_elevated() {
        return Err("windows_service_admin_required".into());
    }
    let gate = native::lifecycle_lock()?;
    let result = super::setup::run(&mut NativeSetup { user_sid }, request);
    drop(gate);
    result
}

pub(super) fn configure(
    request: WindowsServiceConfigureRequest,
) -> Result<WindowsServiceOverview, String> {
    request.validate()?;
    if native::is_elevated() {
        configure_direct(request)?;
    } else {
        const ACTION: &str = "--configure-relay";
        const ERROR: &str = "windows_service_configure_failed";
        let binary = native::installed_binary()?;
        if !binary.is_file() {
            return Err("windows_service_not_installed".into());
        }
        let (id, pipe) = create_elevation_relay()?;
        let arguments = format!("{ACTION} {id} --parent-pid {}", unsafe {
            GetCurrentProcessId()
        });
        let helper = launch_helper(&binary, &arguments)?;
        let pid = unsafe { GetProcessId(helper.0) };
        ipc::relay_to_helper(pipe.0, pid, &request, || {
            require_live_helper(&helper, ERROR, ACTION)
        })?;
        wait_helper(helper, ERROR, Some(ACTION))?;
    }
    status()
}

pub(super) fn set_password(
    request: WindowsServicePasswordRequest,
) -> Result<WindowsServiceOverview, String> {
    request.validate()?;
    if !password_management::current_user_authorized()? {
        return Err("windows_service_password_management_authorization_required".into());
    }
    let _lifecycle_gate = native::lifecycle_lock()?;
    let mut change = NativePasswordChange {
        request: Some(request),
        owns_service: false,
        owned_pid: None,
        supported: false,
    };
    password_management::change_password(&mut change)?;
    status()
}

struct NativePasswordChange {
    request: Option<WindowsServicePasswordRequest>,
    owns_service: bool,
    owned_pid: Option<u32>,
    supported: bool,
}

impl password_management::PasswordChangeOperations for NativePasswordChange {
    fn ensure_authorized(&mut self) -> Result<(), String> {
        if password_management::current_user_authorized()? {
            Ok(())
        } else {
            Err("windows_service_password_management_authorization_required".into())
        }
    }

    fn is_running(&mut self) -> Result<bool, String> {
        super::auto_start::AutoStartOperations::is_running(&mut NativeAutoStart)
    }

    fn start_configuration_only(&mut self) -> Result<(), String> {
        let service = open_service(SERVICE_START | SERVICE_QUERY_STATUS)?
            .ok_or_else(|| "windows_service_not_installed".to_string())?;
        if query(&service)?.dwCurrentState != SERVICE_STOPPED {
            return Err("windows_service_invalid_config".into());
        }
        let flag = native::wide("--configure-only");
        let args = [flag.as_ptr()];
        if unsafe { StartServiceW(service.0, 1, args.as_ptr()) } == 0 {
            return Err(if native::last_error() == 1056 {
                "windows_service_invalid_config"
            } else {
                "windows_service_start_failed"
            }
            .into());
        }
        self.owns_service = true;
        wait_service(&service, SERVICE_RUNNING)?;
        // SCM does not promise a valid PID while START_PENDING. The lifecycle
        // gate covers App callers; service IPC also rejects a normal listener.
        let running = query(&service)?;
        self.owned_pid = Some(password_management::bind_running_instance(
            self.owns_service,
            running.dwCurrentState == SERVICE_RUNNING,
            running.dwProcessId,
        )?);
        Ok(())
    }

    fn password_management_supported(&mut self) -> Result<bool, String> {
        let pid = self
            .owned_pid
            .ok_or_else(|| "windows_service_stop_failed".to_string())?;
        if service_process_id()? != Some(pid) {
            return Err("windows_service_stop_failed".into());
        }
        let overview = ipc::call(&Request::Status)?
            .into_result()?
            .overview
            .ok_or_else(|| "windows_service_ipc_unavailable".to_string())?;
        self.supported = overview.password_management_supported;
        Ok(self.supported)
    }

    fn change_password(&mut self) -> Result<(), String> {
        let request = self
            .request
            .take()
            .ok_or_else(|| "windows_service_internal".to_string())?;
        let expected_pid = self
            .owned_pid
            .ok_or_else(|| "windows_service_stop_failed".to_string())?;
        ipc::call(&Request::SetPassword {
            expected_pid,
            request,
        })
        .and_then(ipc::Response::into_result)
        .map(|_| ())
    }

    fn stop(&mut self) -> Result<(), String> {
        if !self.owns_service {
            return Ok(());
        }
        if !password_management::owns_live_service(
            self.owns_service,
            self.owned_pid,
            service_process_id()?,
        )? {
            return Ok(());
        }
        if self.supported {
            let expected_pid = self
                .owned_pid
                .ok_or_else(|| "windows_service_stop_failed".to_string())?;
            ipc::call(&Request::StopConfigurationOnly { expected_pid })?.into_result()?;
            let deadline = Instant::now() + Duration::from_secs(15);
            while service_process_id()? == Some(expected_pid) {
                if Instant::now() >= deadline {
                    return Err("windows_service_stop_failed".into());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            return Ok(());
        }
        stop_direct()
    }
}

fn wait_service(service: &ScHandle, expected: u32) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let status = query(service)?;
        if status.dwCurrentState == expected {
            return Ok(());
        }
        if Instant::now() >= deadline
            || (expected == SERVICE_RUNNING && status.dwCurrentState == SERVICE_STOPPED)
        {
            return Err(format!(
                "{}:stage=scm_wait;state={};win32_exit_code={};service_exit_code={};checkpoint={}",
                if expected == SERVICE_STOPPED {
                    "windows_service_stop_failed"
                } else {
                    "windows_service_start_failed"
                },
                status.dwCurrentState,
                status.dwWin32ExitCode,
                status.dwServiceSpecificExitCode,
                status.dwCheckPoint
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn start_direct(configure_only: bool) -> Result<(), String> {
    let service = open_service(SERVICE_START | SERVICE_QUERY_STATUS)?
        .ok_or_else(|| "windows_service_not_installed".to_string())?;
    let state = query(&service)?.dwCurrentState;
    if state == SERVICE_RUNNING {
        return Ok(());
    }
    if state == SERVICE_STOP_PENDING {
        wait_service(&service, SERVICE_STOPPED)?;
    }
    let mut flag = native::wide("--configure-only");
    let args = [flag.as_mut_ptr() as *const u16];
    let started = unsafe {
        StartServiceW(
            service.0,
            if configure_only { 1 } else { 0 },
            if configure_only {
                args.as_ptr()
            } else {
                null()
            },
        )
    };
    if started == 0 {
        let error = native::last_error();
        if error != 1056 {
            return Err(format!(
                "windows_service_start_failed:stage=StartServiceW;win32_error={error}"
            ));
        }
    }
    wait_service(&service, SERVICE_RUNNING)
}

fn stop_direct() -> Result<(), String> {
    let Some(service) = open_service(SERVICE_STOP | SERVICE_QUERY_STATUS)? else {
        return Err("windows_service_not_installed".into());
    };
    let state = query(&service)?.dwCurrentState;
    if state == SERVICE_STOPPED {
        return Ok(());
    }
    if state != SERVICE_STOP_PENDING {
        let mut status = SERVICE_STATUS::default();
        if unsafe { ControlService(service.0, SERVICE_CONTROL_STOP, &mut status) } == 0
            && native::last_error() != 1062
        {
            return Err("windows_service_stop_failed".into());
        }
    }
    wait_service(&service, SERVICE_STOPPED)
}

fn configure_direct(request: WindowsServiceConfigureRequest) -> Result<(), String> {
    let _lifecycle_gate = native::lifecycle_lock()?;
    configure_locked(request)
}

fn configure_locked(request: WindowsServiceConfigureRequest) -> Result<(), String> {
    let service = open_service(SERVICE_QUERY_STATUS)?
        .ok_or_else(|| "windows_service_not_installed".to_string())?;
    let was_running = query(&service)?.dwCurrentState == SERVICE_RUNNING;
    if !was_running {
        start_direct(true)?;
    }
    let result = ipc::call(&Request::Configure(request))
        .and_then(ipc::Response::into_result)
        .map(|_| ());
    if !was_running {
        let stop_result = stop_direct();
        if result.is_ok() {
            stop_result?;
        }
    }
    result
}

fn grant_lifecycle_control(user_sid: &str) -> Result<(), String> {
    let service = open_service(READ_CONTROL | WRITE_DAC)?
        .ok_or_else(|| "windows_service_not_installed".to_string())?;
    super::control_access::grant(service.0, user_sid)
}

fn authorize_password_management_direct(user_sid: &str) -> Result<(), String> {
    if !native::is_elevated() {
        return Err("windows_service_admin_required".into());
    }
    let _lifecycle_gate = native::lifecycle_lock()?;
    grant_lifecycle_control(user_sid)?;
    password_management::grant(user_sid)
}

fn install_direct(user_sid: &str) -> Result<(), String> {
    let gate = native::lifecycle_lock()?;
    let result = install_locked(user_sid).and_then(|()| {
        password_management::grant(user_sid)
            .map_err(|_| "windows_service_install_authorization_failed".to_string())
    });
    drop(gate);
    result
}

fn install_locked(user_sid: &str) -> Result<(), String> {
    let source = source_binary()?;
    let previous_state = open_service(SERVICE_QUERY_STATUS)?
        .map(|service| query(&service).map(|status| status.dwCurrentState == SERVICE_RUNNING))
        .transpose()?;
    let was_running = previous_state.unwrap_or(false);
    if previous_state.is_some() {
        stop_direct()?;
    }
    let directory = native::install_directory()?;
    if let Some(parent) = directory.parent() {
        native::protect_directory(parent, true)?;
    }
    native::protect_directory(&directory, true)?;
    let data_directory = native::data_directory()?;
    native::reject_reparse_chain(&data_directory)?;
    if let Some(parent) = data_directory.parent() {
        native::protect_directory(parent, true)?;
    }
    native::protect_directory(&data_directory, true)?;
    let saved_config = data_directory.join("server.json");
    if saved_config.exists() {
        native::protect_file(&saved_config, true)?;
    }
    // Validate saved settings before changing SCM to the new executable.
    let _ = config::read()?;
    let _ = password_management::read_roles()?;
    let target = directory.join(native::BINARY_NAME);
    native::reject_reparse(&target)?;
    if source != target {
        std::fs::copy(&source, &target)
            .map_err(|_| "windows_service_install_copy_failed".to_string())?;
    }
    native::protect_file(&target, true)?;
    let manager = scm(SC_MANAGER_CREATE_SERVICE)?;
    let path = native::wide(format!("\"{}\" --service", target.display()));
    let name = native::wide(native::SERVICE_NAME);
    let display_name = native::wide("LL Remote Server");
    let created = unsafe {
        CreateServiceW(
            manager.0,
            name.as_ptr(),
            display_name.as_ptr(),
            SERVICE_ALL_ACCESS,
            SERVICE_WIN32_OWN_PROCESS,
            SERVICE_DEMAND_START,
            SERVICE_ERROR_NORMAL,
            path.as_ptr(),
            null(),
            null_mut(),
            null(),
            null(),
            null(),
        )
    };
    if created.is_null() {
        if native::last_error() != 1073 {
            return Err("windows_service_install_register_failed".into());
        }
        let existing = open_service(SERVICE_CHANGE_CONFIG)?
            .ok_or_else(|| "windows_service_install_register_failed".to_string())?;
        if unsafe {
            ChangeServiceConfigW(
                existing.0,
                SERVICE_WIN32_OWN_PROCESS,
                SERVICE_NO_CHANGE,
                SERVICE_ERROR_NORMAL,
                path.as_ptr(),
                null(),
                null_mut(),
                null(),
                native::wide("LocalSystem").as_ptr(),
                null(),
                display_name.as_ptr(),
            )
        } == 0
        {
            return Err("windows_service_install_register_failed".into());
        }
    } else {
        drop(ScHandle(created));
    }
    grant_lifecycle_control(user_sid)
        .map_err(|_| "windows_service_install_authorization_failed".to_string())?;
    record_service_app(&source, previous_state.is_none())?;
    if was_running {
        start_direct(false)?;
    }
    Ok(())
}

pub(super) fn prepare_app_update() -> Result<bool, String> {
    let Some(service) = open_service(SERVICE_QUERY_STATUS)? else {
        return Ok(false);
    };
    if query(&service)?.dwCurrentState == SERVICE_STOPPED {
        return Ok(true);
    }
    match lifecycle_action("--stop") {
        Ok(()) => {}
        Err(error)
            if matches!(
                error.as_str(),
                "windows_service_access_denied" | "windows_service_admin_required"
            ) && !native::is_elevated() =>
        {
            // Use the package's helper for this stop. Repairing the application
            // doesn't require granting its operator permanent service control.
            let helper = launch_helper(&source_binary()?, "--stop")?;
            wait_helper(helper, "windows_service_stop_failed", Some("--stop"))?;
        }
        Err(error) => return Err(error),
    }
    Ok(true)
}

/// Read SCM only: do not load configuration or touch credentials during preflight.
pub(super) fn installer_state() -> Result<Option<bool>, String> {
    open_service(SERVICE_QUERY_STATUS)?
        .map(|service| query(&service).map(|state| state.dwCurrentState != SERVICE_STOPPED))
        .transpose()
}

fn service_app_file() -> Result<std::path::PathBuf, String> {
    // Keep ownership outside the data tree: a failed data purge must not lose
    // the information needed to retry that purge safely.
    Ok(native::install_directory()?.join("app-installation.json"))
}

fn record_service_app(source: &std::path::Path, fresh_service: bool) -> Result<(), String> {
    let path = service_app_file()?;
    native::reject_reparse_chain(&path)?;
    // Keep the creating installation when another user updates the shared service.
    if path
        .try_exists()
        .map_err(|_| "windows_service_access_denied")?
    {
        native::verify_protected_metadata(&path)?;
        if !fresh_service {
            let owner: std::path::PathBuf = serde_json::from_slice(
                &std::fs::read(&path).map_err(|_| "windows_service_install_failed")?,
            )
            .map_err(|_| "windows_service_install_failed")?;
            if !owner.is_absolute() {
                return Err("windows_service_install_failed".into());
            }
            return Ok(());
        }
        // A newly created service belongs to its new creating installation,
        // even if a prior failed removal left an old ownership receipt.
    }
    let directory = source.parent().ok_or("windows_service_binary_missing")?;
    let bytes = serde_json::to_vec(directory).map_err(|_| "windows_service_install_failed")?;
    use std::io::Write;
    let pending = path.with_extension(format!("{}.next", uuid::Uuid::new_v4()));
    native::reject_reparse_chain(&pending)?;
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)
            .map_err(|_| "windows_service_install_failed")?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "windows_service_install_failed")?;
        drop(file);
        native::protect_file(&pending, true)?;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        if unsafe {
            MoveFileExW(
                native::wide(&pending).as_ptr(),
                native::wide(&path).as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err("windows_service_install_failed".to_string());
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(pending);
    }
    result
}

pub(super) fn app_owns_service(directory: &std::path::Path) -> Result<bool, String> {
    let path = service_app_file()?;
    native::reject_reparse_chain(&path)?;
    if !path
        .try_exists()
        .map_err(|_| "windows_service_access_denied")?
    {
        return if open_service(SERVICE_QUERY_STATUS)?.is_some() {
            Err("windows_uninstall_service_owner_unknown".into())
        } else {
            Ok(false)
        };
    }
    native::verify_protected_metadata(&path)?;
    let metadata = std::fs::metadata(&path).map_err(|_| "windows_service_access_denied")?;
    if !metadata.is_file() || metadata.len() > 64 * 1024 {
        return Err("windows_uninstall_service_owner_unknown".into());
    }
    let owner: std::path::PathBuf =
        serde_json::from_slice(&std::fs::read(path).map_err(|_| "windows_service_access_denied")?)
            .map_err(|_| "windows_uninstall_service_owner_unknown")?;
    Ok(crate::windows_uninstall::same_install_directory(
        &owner, directory,
    ))
}

fn remove_service_app() -> Result<(), String> {
    let path = service_app_file()?;
    native::reject_reparse_chain(&path)?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("windows_service_uninstall_failed".into()),
    }
}

pub(super) fn app_requires_uninstall_authorization(
    directory: &std::path::Path,
) -> Result<bool, String> {
    if open_service(SERVICE_QUERY_STATUS)?.is_none() {
        return Ok(false);
    }
    app_owns_service(directory)
}

pub(super) fn prepare_app_uninstall(
    delete_data: bool,
    directory: &std::path::Path,
) -> Result<crate::windows_uninstall::report::CleanupReport, String> {
    use crate::windows_uninstall::report::CleanupReport;
    let active_installation = open_service(SERVICE_QUERY_STATUS)?.is_some();
    let owns = match app_owns_service(directory) {
        Ok(owns) => owns,
        Err(error) if !active_installation => {
            let mut report = CleanupReport::default();
            report.warning(&error, native::install_directory()?);
            return Ok(report);
        }
        Err(error) => return Err(error),
    };
    if !owns {
        return Ok(CleanupReport::default());
    }
    if !native::is_elevated() {
        if active_installation {
            return Err("windows_uninstall_service_admin_required".into());
        }
        let mut report = CleanupReport::default();
        report.warning(
            "windows_uninstall_service_access_denied",
            native::install_directory()?,
        );
        return Ok(report);
    }
    uninstall_collect(delete_data, Some(directory))
}

fn uninstall_direct(delete_data: bool, app: Option<&std::path::Path>) -> Result<(), String> {
    let report = uninstall_collect(delete_data, app)?;
    // Preserve the standalone service-management command's existing error
    // contract. App uninstall consumes the full, nonfatal residue report.
    if let Some(item) = report.warnings.first() {
        return Err(crate::windows_uninstall::error_from_exit(item.code as u32));
    }
    Ok(())
}

fn uninstall_collect(
    delete_data: bool,
    app: Option<&std::path::Path>,
) -> Result<crate::windows_uninstall::report::CleanupReport, String> {
    use crate::windows_uninstall::report::CleanupReport;
    let mut report = CleanupReport::default();
    let lifecycle_gate = native::lifecycle_lock()?;
    if let Some(directory) = app {
        if !app_owns_service(directory)? {
            return Ok(report);
        }
    }
    let service =
        open_service(SERVICE_QUERY_STATUS | windows_sys::Win32::Storage::FileSystem::DELETE)?;
    if let Some(service) = service {
        let was_running = query(&service)?.dwCurrentState == SERVICE_RUNNING;
        let reset = if was_running || start_direct(true).is_ok() {
            ipc::call(&Request::Reset)
                .and_then(ipc::Response::into_result)
                .map(|_| ())
        } else {
            Err("windows_uninstall_service_credential_failed".into())
        };
        // Failure to wipe credentials must not prevent stopping and deleting
        // the service. Failure to stop/delete the service remains fatal.
        report.merge(crate::windows_uninstall::report::remove_service_core(
            reset,
            stop_direct,
            || {
                if unsafe { DeleteService(service.0) } == 0 {
                    Err("windows_service_uninstall_failed".into())
                } else {
                    Ok(())
                }
            },
            &native::data_directory()?,
        )?);
        drop(service);
    }
    report.capture(
        password_management::remove()
            .map_err(|error| format!("windows_uninstall_service_credential_failed:{error}")),
        native::install_directory()?,
    );
    report.capture(
        config::remove().map_err(|error| format!("windows_uninstall_data_cleanup_failed:{error}")),
        native::data_directory()?,
    );
    let binary = native::installed_binary()?;
    native::reject_reparse(&binary)?;
    if binary.exists() && std::fs::remove_file(&binary).is_err() {
        if unsafe {
            MoveFileExW(
                native::wide(&binary).as_ptr(),
                null(),
                MOVEFILE_DELAY_UNTIL_REBOOT,
            )
        } == 0
        {
            return Err("windows_uninstall_files_failed".into());
        }
        report.reboot = true;
    }
    drop(lifecycle_gate);
    if delete_data {
        let directory = native::data_directory()?;
        report.capture(
            crate::windows_uninstall::data::remove_tree(&directory)
                .map_err(|error| crate::windows_uninstall::data::cleanup_error(&error)),
            &directory,
        );
        if let Some(parent) = directory.parent() {
            if parent.file_name().is_some_and(|name| name == "LLRemote") {
                let _ = std::fs::remove_dir(parent);
            }
        }
    }
    report.capture(
        remove_service_app()
            .map_err(|error| format!("windows_uninstall_data_cleanup_failed:{error}")),
        service_app_file()?,
    );
    let _ = std::fs::remove_dir(native::install_directory()?);
    Ok(report)
}

fn direct_action(verb: &str) -> Result<(), String> {
    if matches!(verb, "--start" | "--stop") {
        return lifecycle_action(verb);
    }
    if !native::is_elevated() {
        return Err("windows_service_admin_required".into());
    }
    match verb {
        "--install" => install_direct(&native::current_user_sid()?),
        "--uninstall" => uninstall_direct(false, None),
        "--enable-autostart" => set_auto_start_direct(true),
        "--disable-autostart" => set_auto_start_direct(false),
        "--authorize-password-management" => {
            authorize_password_management_direct(&native::current_user_sid()?)
        }
        "--enable-sas" => {
            if open_service(SERVICE_QUERY_STATUS)?.is_none() {
                return Err("windows_service_not_installed".into());
            }
            super::sas::enable_service_policy()
        }
        _ => Err("windows_service_invalid_config".into()),
    }
}

fn lifecycle_action(verb: &str) -> Result<(), String> {
    let _lifecycle_gate = native::lifecycle_lock()?;
    match verb {
        "--start" => start_direct(false),
        "--stop" => stop_direct(),
        _ => Err("windows_service_invalid_config".into()),
    }
}

pub(super) fn run_from_args() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // runas cannot inherit the NSIS handle. Reopen a restricted lifetime lease
    // before any installer-triggered mutation, retaining it until this helper
    // exits even when its parent has already reported a timeout.
    let _installer_lease = if args.first().is_some_and(|verb| {
        matches!(
            verb.as_str(),
            "--install" | "--install-relay" | "--stop" | "--start"
        )
    }) {
        native::installer_lease()?
    } else {
        None
    };
    match args.as_slice() {
        [verb, flag, directory] if verb == "--uninstall" && flag == "--app-directory" => {
            if !native::is_elevated() {
                return Err("windows_service_admin_required".into());
            }
            uninstall_direct(false, Some(std::path::Path::new(directory)))
        }
        [verb, flag, directory, purge]
            if verb == "--uninstall" && flag == "--app-directory" && purge == "--purge-data" =>
        {
            if !native::is_elevated() {
                return Err("windows_service_admin_required".into());
            }
            uninstall_direct(true, Some(std::path::Path::new(directory)))
        }
        [verb, flag] if verb == "--uninstall" && flag == "--purge-data" => {
            if !native::is_elevated() {
                return Err("windows_service_admin_required".into());
            }
            uninstall_direct(true, None)
        }
        [verb] if verb == "--service" => supervisor::run_service(),
        [verb, id, flag, pid] if verb == "--repair-sharing-relay" && flag == "--parent-pid" => {
            if !native::is_elevated() {
                return Err("windows_service_admin_required".into());
            }
            let pid = pid
                .parse()
                .map_err(|_| "windows_service_access_denied".to_string())?;
            let user_sid = native::operator_user_sid(pid)?;
            ipc::send_repair_report(id, pid, || {
                run_service_repair(user_sid, &repair_update_required())
            })
        }
        [verb] if verb == "--status" => {
            let overview = status()?;
            println!(
                "{}",
                serde_json::to_string(&overview)
                    .map_err(|_| "windows_service_internal".to_string())?
            );
            Ok(())
        }
        [verb, flag, pid]
            if matches!(
                verb.as_str(),
                "--install" | "--start" | "--stop" | "--authorize-password-management"
            ) && flag == "--operator-pid" =>
        {
            if !native::is_elevated() {
                return Err("windows_service_admin_required".into());
            }
            let user_sid = native::operator_user_sid(
                pid.parse()
                    .map_err(|_| "windows_service_access_denied".to_string())?,
            )?;
            if verb == "--install" {
                install_direct(&user_sid)
            } else if verb == "--authorize-password-management" {
                authorize_password_management_direct(&user_sid)
            } else {
                grant_lifecycle_control(&user_sid)?;
                lifecycle_action(verb)
            }
        }
        [verb]
            if matches!(
                verb.as_str(),
                "--install"
                    | "--uninstall"
                    | "--start"
                    | "--stop"
                    | "--enable-sas"
                    | "--enable-autostart"
                    | "--disable-autostart"
                    | "--authorize-password-management"
            ) =>
        {
            direct_action(verb)
        }
        [verb, flag, session] if verb == "--worker" && flag == "--session-id" => worker::run(
            session
                .parse()
                .map_err(|_| "windows_service_invalid_config".to_string())?,
        ),
        [verb, flag, session] if verb == "--user-agent" && flag == "--session-id" => {
            super::user_agent::run(
                session
                    .parse()
                    .map_err(|_| "windows_service_invalid_config".to_string())?,
            )
        }
        [verb, id, flag, pid] if verb == "--install-relay" && flag == "--parent-pid" => {
            if !native::is_elevated() {
                return Err("windows_service_admin_required".into());
            }
            let pid = pid
                .parse()
                .map_err(|_| "windows_service_access_denied".to_string())?;
            let user_sid = native::operator_user_sid(pid)?;
            let request = ipc::receive_relay(id, pid)?;
            setup_direct(request, &user_sid)
        }
        [verb, id, flag, pid] if verb == "--configure-relay" && flag == "--parent-pid" => {
            if !native::is_elevated() {
                return Err("windows_service_admin_required".into());
            }
            let request = ipc::receive_relay(
                id,
                pid.parse()
                    .map_err(|_| "windows_service_access_denied".to_string())?,
            )?;
            configure_direct(request)
        }
        _ => Err("windows_service_invalid_config".into()),
    }
}
