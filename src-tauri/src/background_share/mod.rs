//! User background sharing reuses the existing RFB/Agent engine. App exit owns
//! only its viewers; this process has a separate lifetime.
#[cfg(target_os = "macos")]
mod bundle_macos;
mod config;
pub(crate) mod disconnect_lock;
mod host;
#[cfg(target_os = "macos")]
#[path = "ipc_macos.rs"]
mod ipc;
mod paths;
#[cfg(target_os = "macos")]
mod permission_identity;
#[cfg(any(target_os = "windows", test))]
mod repair;
mod startup;
#[cfg(target_os = "windows")]
mod startup_windows;
mod types;
#[cfg(any(target_os = "windows", test))]
mod windows_context;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod ipc {
    use super::types::{Reply, Request};
    pub fn call(_: &Request) -> Result<Reply, String> {
        Err("background_share_unsupported".into())
    }
    pub struct Server;
    impl Server {
        pub fn new() -> Result<Self, String> {
            Err("background_share_unsupported".into())
        }
        pub fn poll(&self, _: impl FnOnce(Request) -> Reply) -> Result<(), String> {
            Err("background_share_unsupported".into())
        }
    }
}
#[cfg(target_os = "windows")]
#[path = "ipc_windows.rs"]
mod ipc;

use types::Request;
pub use types::{BackgroundShareOverview, BackgroundShareStartRequest};
use types::{BackgroundSharePermissionSource, StartupRegistration};

const BUILD_ID: &str = env!("VNC_SHARING_BUILD_ID");
#[cfg(target_os = "windows")]
const IPC_VERSION: u32 = 9;
#[cfg(not(target_os = "windows"))]
const IPC_VERSION: u32 = 6;
// Keep the first supported host protocol when adding a compatible revision.
#[cfg(target_os = "windows")]
const MIN_SUPPORTED_IPC_VERSION: u32 = 9;
#[cfg(not(target_os = "windows"))]
const MIN_SUPPORTED_IPC_VERSION: u32 = 6;
static HOST_CONTROL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lock_host_control() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    HOST_CONTROL
        .lock()
        .map_err(|_| "background_share_runtime_failed".into())
}

#[cfg(target_os = "windows")]
pub(crate) fn stop_for_user_uninstall(sid: &str, local: &std::path::Path) -> Result<(), String> {
    let _control = lock_host_control()?;
    ipc::stop_for_uninstall_user(sid, local)
}

#[cfg(target_os = "windows")]
pub(crate) fn installer_has_runtime() -> Result<bool, String> {
    ipc::installer_has_runtime()
}

#[cfg(target_os = "windows")]
pub(crate) fn prepare_update() -> Result<(), String> {
    let _control = lock_host_control()?;
    // Unlike uninstall, keep the saved login policy and registration.
    ipc::stop_for_uninstall()
}

#[cfg(target_os = "windows")]
pub(crate) fn finish_update() -> Result<(), String> {
    let _control = lock_host_control()?;
    if config::Config::load()?.auto_start && startup::registration() == StartupRegistration::Enabled
    {
        startup::configure(true)?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub(crate) fn delete_saved_password() -> Result<(), String> {
    config::restore_password(None)
}

/// Backend-only credential reuse. Never return the saved secret to a WebView.
#[cfg(target_os = "windows")]
pub(crate) fn password_for_service_setup() -> Result<zeroize::Zeroizing<String>, String> {
    config::read_password(&config::Config::load()?)
}

// A foreground fallback must never masquerade as the capture process's grant.
fn normalize_overview(mut overview: BackgroundShareOverview) -> BackgroundShareOverview {
    overview.current_build_id = BUILD_ID.into();
    overview.restart_supported = cfg!(target_os = "macos");
    overview.startup_registration = startup::registration();
    overview.last_login_attempt = paths::data_directory()
        .ok()
        .and_then(|directory| crate::startup_attempt::read(&directory));
    if !overview.process_running {
        overview.permission_source = BackgroundSharePermissionSource::Pending;
        overview.runtime = None;
        overview.signature_changed_permissions.clear();
        #[cfg(target_os = "macos")]
        {
            overview.permission_identity = None;
        }
    }
    if overview.permission_source == BackgroundSharePermissionSource::Pending {
        for permission in &mut overview.server.permissions {
            permission.status = crate::local_vnc_server::LocalVncServerPermissionStatus::Unknown;
        }
    }
    overview
}

fn validate_host_protocol(overview: &BackgroundShareOverview) -> Result<(), String> {
    if !(MIN_SUPPORTED_IPC_VERSION..=IPC_VERSION).contains(&overview.ipc_version) {
        return Err("background_share_protocol_incompatible".into());
    }
    Ok(())
}

// A saved login task must not start a second listener beside an installed service.
fn validate_background_runtime() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    if crate::windows_service::status()?.installed {
        return Err("background_share_service_installed".into());
    }
    Ok(())
}

fn ensure_host() -> Result<(), String> {
    ensure_host_with_actions(control_status, startup::launch, |process| {
        process.check_running()
    })
}

/// Explicit ordinary-sharing repair prepares an idle host. It never installs a
/// service, changes SAS policy, or starts a public listener.
#[cfg(target_os = "windows")]
pub(crate) fn repair_sharing() -> Result<crate::windows_service::WindowsServiceRepairReport, String>
{
    let _control = lock_host_control()?;
    validate_background_runtime()?;
    let items = repair::run(control_status, ensure_host, || restart_host(true));
    Ok(crate::windows_service::WindowsServiceRepairReport {
        overview: crate::windows_service::status()?,
        items,
    })
}

fn ensure_host_with_actions<P>(
    mut status: impl FnMut() -> Result<BackgroundShareOverview, String>,
    launch: impl FnOnce() -> Result<P, String>,
    mut check_process: impl FnMut(&P) -> Result<(), String>,
) -> Result<(), String> {
    match status() {
        Ok(overview) => return validate_host_protocol(&overview),
        Err(code) if code == "background_share_ipc_unavailable" => {}
        Err(code) => return Err(code),
    }
    let process = launch()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while std::time::Instant::now() < deadline {
        match status() {
            Ok(overview) => return validate_host_protocol(&overview),
            Err(code)
                if matches!(
                    code.as_str(),
                    "background_share_ipc_unavailable" | "background_share_ipc_busy"
                ) =>
            {
                check_process(&process)?;
            }
            Err(code) => return Err(code),
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Err("background_share_startup_unavailable".into())
}

fn control_status() -> Result<BackgroundShareOverview, String> {
    require_running_host(runtime_status()?)
}

fn require_running_host(
    overview: BackgroundShareOverview,
) -> Result<BackgroundShareOverview, String> {
    // Display queries may return saved settings when IPC is absent. Lifecycle
    // control requires a live host, including while waiting for launch or exit.
    if !overview.process_running {
        return Err("background_share_ipc_unavailable".into());
    }
    Ok(overview)
}

pub fn status() -> Result<BackgroundShareOverview, String> {
    read_status(&Request::Status)
}

pub fn runtime_status() -> Result<BackgroundShareOverview, String> {
    read_status(&Request::RuntimeStatus)
}

fn read_status(request: &Request) -> Result<BackgroundShareOverview, String> {
    match ipc::call(request) {
        Ok(reply) => reply.into_result().map(normalize_overview),
        Err(code) if code == "background_share_ipc_unavailable" => {
            let config = config::Config::load()?;
            let mut server = crate::local_vnc_server::pending_local_vnc_server_overview();
            if config.port > 0 {
                server.port = config.port;
            }
            if !config.bind_address.is_empty() {
                server.bind_address = config.bind_address.clone();
            }
            server.view_only = config.view_only;
            Ok(normalize_overview(BackgroundShareOverview {
                server,
                service_fallback: false,
                temporary_share: false,
                configured: config.port > 0,
                credential_configured: config.credential_ref.is_some(),
                auto_start: config.auto_start,
                startup_registration: StartupRegistration::Unknown,
                last_login_attempt: None,
                process_running: false,
                last_error_code: None,
                runtime: None,
                current_build_id: BUILD_ID.into(),
                permission_source: BackgroundSharePermissionSource::Pending,
                restart_supported: cfg!(target_os = "macos"),
                ipc_version: IPC_VERSION,
                #[cfg(target_os = "macos")]
                permission_identity: None,
                signature_changed_permissions: vec![],
            }))
        }
        Err(code) => Err(code),
    }
}

pub fn start(request: BackgroundShareStartRequest) -> Result<BackgroundShareOverview, String> {
    let _control = lock_host_control()?;
    validate_background_runtime()?;
    ensure_host()?;
    if host_requires_write_upgrade(&control_status()?)? {
        restart_host(true)?;
    }
    ipc::call(&Request::Start(request))?
        .into_result()
        .map(normalize_overview)
}

#[cfg(target_os = "windows")]
pub(crate) fn start_service_fallback(
    mut request: BackgroundShareStartRequest,
) -> Result<BackgroundShareOverview, String> {
    let _control = lock_host_control()?;
    request.temporary_password = true;
    ensure_host()?;
    if host_requires_write_upgrade(&control_status()?)? {
        restart_host(true)?;
    }
    ipc::call(&Request::StartServiceFallback(request))?
        .into_result()
        .map(normalize_overview)
}

pub fn stop() -> Result<BackgroundShareOverview, String> {
    let _control = lock_host_control()?;
    #[cfg(target_os = "macos")]
    {
        stop_host(false)?;
        runtime_status()
    }
    #[cfg(not(target_os = "macos"))]
    ipc::call(&Request::Stop)?
        .into_result()
        .map(normalize_overview)
}

/// Changing the login policy is independent of starting a listener. Upgrade an
/// idle host only at this explicit action; a running listener stays owned.
pub fn set_auto_start(enabled: bool) -> Result<BackgroundShareOverview, String> {
    let _control = lock_host_control()?;
    if enabled {
        validate_background_runtime()?;
    }
    ensure_host()?;
    if host_requires_write_upgrade(&control_status()?)? {
        restart_host(true)?;
    }
    ipc::call(&Request::SetAutoStart { enabled })?
        .into_result()
        .map(normalize_overview)
}

// Writes require the current protocol; compatible older builds can be inspected and stopped.
fn host_requires_write_upgrade(overview: &BackgroundShareOverview) -> Result<bool, String> {
    validate_host_protocol(overview)?;
    if overview.ipc_version == IPC_VERSION {
        return Ok(false);
    }
    if overview.server.status != crate::local_vnc_server::LocalVncServerStatus::Stopped {
        return Err("background_share_runtime_outdated".into());
    }
    Ok(true)
}

/// Explicit permission recovery stops sharing first. It never retains a
/// transient password or automatically resumes a listener after restarting.
pub fn restart() -> Result<BackgroundShareOverview, String> {
    #[cfg(not(target_os = "macos"))]
    return Err("background_share_unsupported".into());
    #[cfg(target_os = "macos")]
    {
        let _control = lock_host_control()?;
        restart_host(false)
    }
}

#[cfg(any(target_os = "macos", test))]
fn idle_host_needs_update(overview: &BackgroundShareOverview) -> bool {
    overview.process_running
        && overview.server.status == crate::local_vnc_server::LocalVncServerStatus::Stopped
        && (overview.ipc_version < IPC_VERSION
            || overview
                .runtime
                .as_ref()
                .is_some_and(|runtime| runtime.build_id != BUILD_ID))
}

/// Preparation upgrades idle old builds using atomic Recover. Active sharing
/// remains owned by its current host until the user confirms maintenance.
pub fn prepare(recover: bool) -> Result<BackgroundShareOverview, String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = recover;
        Err("background_share_unsupported".into())
    }
    #[cfg(target_os = "macos")]
    {
        let _control = lock_host_control()?;
        let overview = status()?;
        if overview.server.status == crate::local_vnc_server::LocalVncServerStatus::Running {
            return Ok(overview);
        }
        if recover || idle_host_needs_update(&overview) {
            return match restart_host(true) {
                Err(code) if code == "background_share_already_running" => status(),
                Ok(view) => {
                    if view.auto_start && startup::registration() == StartupRegistration::Enabled {
                        startup::configure(true)?;
                    }
                    Ok(view)
                }
                Err(code) => Err(code),
            };
        }
        ensure_host()?;
        status()
    }
}

fn restart_host(only_if_idle: bool) -> Result<BackgroundShareOverview, String> {
    stop_host(only_if_idle)?;
    ensure_host()?;
    status()
}

fn stop_host(only_if_idle: bool) -> Result<(), String> {
    let mut pid = None;
    match control_status() {
        Ok(overview) => {
            pid = overview.runtime.as_ref().map(|runtime| runtime.pid);
            if only_if_idle
                && overview.server.status == crate::local_vnc_server::LocalVncServerStatus::Running
            {
                return Err("background_share_already_running".into());
            }
            let request = if only_if_idle {
                Request::Recover
            } else {
                Request::Stop
            };
            ipc::call(&request)?.into_result()?;
        }
        Err(code) if code == "background_share_ipc_unavailable" => {}
        Err(code) => return Err(code),
    }
    wait_for_host_exit(
        || match control_status() {
            Err(code) if code == "background_share_ipc_unavailable" => Ok(false),
            Err(code) => Err(code),
            Ok(_) => Ok(true),
        },
        || {
            #[cfg(target_os = "macos")]
            return pid.is_some_and(ipc::process_running);
            #[cfg(not(target_os = "macos"))]
            {
                let _ = pid;
                false
            }
        },
        startup::unload,
    )
}

fn wait_for_host_exit(
    mut endpoint_present: impl FnMut() -> Result<bool, String>,
    mut process_running: impl FnMut() -> bool,
    unload: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        if !endpoint_present()? && !process_running() {
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Err("background_share_restart_timeout".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    unload()
}

pub fn update_view(
    request: crate::local_vnc_server::LocalVncServerViewRequest,
) -> Result<BackgroundShareOverview, String> {
    ipc::call(&Request::View(request))?
        .into_result()
        .map(normalize_overview)
}

pub fn disconnect(connection_id: String) -> Result<BackgroundShareOverview, String> {
    ipc::call(&Request::Disconnect { connection_id })?
        .into_result()
        .map(normalize_overview)
}

/// Permission prompts must originate in the process that captures and injects
/// input; the foreground App may have a different macOS TCC identity.
/// On macOS, reset a missing grant for the verified bundle before requesting it.
pub fn request_permission(
    kind: crate::local_vnc_server::LocalVncServerPermissionKind,
) -> Result<crate::local_vnc_server::LocalVncServerPermissionRequestResult, String> {
    let _control = lock_host_control()?;
    ensure_host()?;
    #[cfg(target_os = "macos")]
    {
        let granted = |overview: &BackgroundShareOverview| {
            overview.server.permissions.iter().any(|permission| {
                permission.kind == kind
                    && permission.status
                        == crate::local_vnc_server::LocalVncServerPermissionStatus::Granted
            })
        };
        let already_granted = || crate::local_vnc_server::LocalVncServerPermissionRequestResult {
            opened: false,
            kind: kind.clone(),
            platform: "macos".into(),
            target: "not_required".into(),
            message: "Permission is already granted.".into(),
        };
        let mut overview = status()?;
        if granted(&overview) {
            return Ok(already_granted());
        }
        if permission_host_needs_refresh(&overview)? {
            // Refresh only an idle outdated host; a running listener is preserved.
            restart_host(true)?;
            overview = status()?;
            if granted(&overview) {
                return Ok(already_granted());
            }
        }
        if overview.server.status != crate::local_vnc_server::LocalVncServerStatus::Stopped {
            return Err("background_share_permission_repair_busy".into());
        }
        let executable = bundle_macos::executable()?;
        let owner_identifier =
            if kind == crate::local_vnc_server::LocalVncServerPermissionKind::ScreenRecording {
                bundle_macos::owning_application_identifier(&executable)?
            } else {
                None
            };
        permission_identity::repair_with_actions(
            &overview,
            &kind,
            &bundle_macos::associated_identifier(&executable)?,
            || stop_host(true),
            |kind, identifier| {
                permission_identity::register_bundle(&executable)?;
                permission_identity::reset_for_host(kind, identifier, owner_identifier.as_deref())
            },
            ensure_host,
        )?;
    }
    ipc::call(&Request::Permission(kind))?.into_permission_result()
}

#[cfg(target_os = "macos")]
fn permission_host_needs_refresh(overview: &BackgroundShareOverview) -> Result<bool, String> {
    if overview
        .runtime
        .as_ref()
        .is_some_and(|runtime| runtime.build_id == BUILD_ID)
    {
        return Ok(false);
    }
    if overview.server.status == crate::local_vnc_server::LocalVncServerStatus::Running {
        return Err("background_share_runtime_outdated".into());
    }
    Ok(true)
}

/// Returns true only for the dedicated background mode, before any WebView or
/// application .env initialization. No credential appears in the arguments.
pub fn run_if_requested() -> Result<bool, String> {
    let Some(mode) = types::BackgroundShareLaunchMode::from_arguments(
        &std::env::args().skip(1).collect::<Vec<_>>(),
    ) else {
        return Ok(false);
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return Err("background_share_unsupported".into());
    #[cfg(target_os = "windows")]
    {
        if startup_windows::relaunch_if_elevated(mode.argument())? {
            return Ok(true);
        }
        crate::windows_service::initialize_user_background()?;
    }
    #[cfg(target_os = "macos")]
    {
        let source = std::env::current_exe()
            .map_err(|_| "background_share_binary_unavailable".to_string())?;
        if bundle_macos::packaged_executable(&source).as_ref() != Some(&source) {
            return Err("background_share_binary_unavailable".into());
        }
        unsafe extern "C" {
            fn vnc_sharing_host_run_loop();
            fn vnc_sharing_host_stop_loop();
        }
        let server = std::thread::spawn(move || {
            let result = host::run(mode);
            unsafe { vnc_sharing_host_stop_loop() };
            result
        });
        unsafe { vnc_sharing_host_run_loop() };
        server
            .join()
            .map_err(|_| "background_share_runtime_failed".to_string())??;
    }
    #[cfg(target_os = "windows")]
    host::run(mode)?;
    Ok(true)
}

#[cfg(test)]
#[path = "../../tests/support/background_share_lifecycle_tests.rs"]
mod lifecycle_tests;
