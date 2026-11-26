//! Windows-only supervised sharing. The service owns lifecycle/SAS; its trusted
//! console-session worker owns the existing RFB server and native desktop access.
#[cfg(any(target_os = "windows", test))]
pub(crate) mod fallback;
mod types;
#[cfg(any(target_os = "windows", test))]
pub(crate) use types::{RepairKind, RepairStatus, WindowsServiceRepairItem};
pub use types::{
    ServiceStartOutcome, WindowsServiceConfigureRequest, WindowsServiceOverview,
    WindowsServicePasswordRequest, WindowsServiceRepairReport,
};

#[cfg(any(target_os = "windows", test))]
const IPC_VERSION: u32 = 2;
#[cfg(target_os = "windows")]
const MIN_SUPPORTED_IPC_VERSION: u32 = 2;

#[cfg(any(target_os = "windows", test))]
mod auto_start;
#[cfg(any(target_os = "windows", test))]
mod clipboard_transfer;
#[cfg(target_os = "windows")]
mod config;
#[cfg(target_os = "windows")]
mod control_access;
#[cfg(target_os = "windows")]
mod credentials;
#[cfg(target_os = "windows")]
mod install;
#[cfg(target_os = "windows")]
pub(crate) mod ipc;
#[cfg(any(target_os = "windows", test))]
mod maintenance;
#[cfg(target_os = "windows")]
pub(crate) mod native;
#[cfg(any(target_os = "windows", test))]
mod password_management;
#[cfg(any(target_os = "windows", test))]
mod repair;
#[cfg(target_os = "windows")]
mod sas;
#[cfg(any(target_os = "windows", test))]
mod setup;
#[cfg(any(target_os = "windows", test))]
mod storage_layout;
#[cfg(target_os = "windows")]
mod supervisor;
#[cfg(target_os = "windows")]
mod uac;
#[cfg(target_os = "windows")]
mod user_agent;
#[cfg(target_os = "windows")]
pub(crate) mod user_token;
#[cfg(target_os = "windows")]
mod worker;

/// Forward a public RFB clipboard message through the service to the logged-on user.
#[cfg(target_os = "windows")]
pub(crate) fn write_clipboard_from_broker(text: &str) -> Result<(), String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    const CHUNK_BYTES: usize = 10 * 1024;
    if text.len() > 1024 * 1024 {
        return Err("clipboard_resource_limit".into());
    }
    let transfer_id = uuid::Uuid::new_v4().to_string();
    for (index, chunk) in text.as_bytes().chunks(CHUNK_BYTES).enumerate() {
        ipc::call(&ipc::Request::BrokerClipboardChunk {
            transfer_id: transfer_id.clone(),
            offset: (index * CHUNK_BYTES) as u32,
            total: text.len() as u32,
            data: STANDARD.encode(chunk),
        })?
        .into_result()?;
    }
    if text.is_empty() {
        ipc::call(&ipc::Request::BrokerClipboardChunk {
            transfer_id,
            offset: 0,
            total: 0,
            data: String::new(),
        })?
        .into_result()?;
    }
    Ok(())
}

pub fn status() -> Result<WindowsServiceOverview, String> {
    #[cfg(target_os = "windows")]
    {
        install::status()
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(WindowsServiceOverview::unsupported())
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn prepare_app_uninstall(
    delete_data: bool,
    directory: &std::path::Path,
) -> Result<crate::windows_uninstall::report::CleanupReport, String> {
    install::prepare_app_uninstall(delete_data, directory)
}

#[cfg(target_os = "windows")]
pub(crate) fn validate_app_uninstall(directory: &std::path::Path) -> Result<bool, String> {
    install::app_requires_uninstall_authorization(directory)
}

#[cfg(target_os = "windows")]
pub(crate) fn prepare_app_update() -> Result<bool, String> {
    install::prepare_app_update()
}

#[cfg(target_os = "windows")]
pub(crate) fn installer_state() -> Result<Option<bool>, String> {
    install::installer_state()
}

/// Shares one Server data model across service and user background
/// runtimes. A stopped service contributes settings, never synthetic frames.
pub fn server_status() -> Result<crate::local_vnc_server::LocalVncServerOverview, String> {
    let service = status()?;
    if !service.supported {
        return Err("windows_service_unsupported".into());
    }
    if let Some(server) = service.server {
        return Ok(server);
    }
    let mut server = crate::local_vnc_server::LocalVncServerManager::default().status()?;
    server.port = service.port.unwrap_or(5904);
    if let Some(address) = service.bind_address {
        server.bind_address = address;
    }
    server.view_only = service.view_only;
    server.status = match service.status.as_str() {
        "running" | "start_pending" => crate::local_vnc_server::LocalVncServerStatus::Starting,
        "stop_pending" => crate::local_vnc_server::LocalVncServerStatus::Stopping,
        "stopped" | "not_installed" => crate::local_vnc_server::LocalVncServerStatus::Stopped,
        _ => crate::local_vnc_server::LocalVncServerStatus::Unknown,
    };
    Ok(server)
}

pub fn disconnect_connection(
    connection_id: String,
) -> Result<crate::local_vnc_server::LocalVncServerOverview, String> {
    #[cfg(target_os = "windows")]
    {
        let service = ipc::call(&ipc::Request::Status)?
            .into_result()?
            .overview
            .ok_or_else(|| "windows_service_ipc_unavailable".to_string())?;
        if !service.supports_connection_disconnect() {
            return Err("windows_service_protocol_incompatible".into());
        }
        ipc::call(&ipc::Request::DisconnectConnection { connection_id })?
            .into_result()?
            .overview
            .and_then(|overview| overview.server)
            .ok_or_else(|| "windows_service_ipc_unavailable".to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        drop(connection_id);
        Err("windows_service_unsupported".into())
    }
}

macro_rules! service_action {
    ($name:ident, $verb:literal) => {
        pub fn $name() -> Result<WindowsServiceOverview, String> {
            #[cfg(target_os = "windows")]
            {
                install::perform_action($verb)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err("windows_service_unsupported".to_string())
            }
        }
    };
}
pub fn install(
    request: Option<WindowsServiceConfigureRequest>,
) -> Result<WindowsServiceOverview, String> {
    #[cfg(target_os = "windows")]
    {
        install::install(request)
    }
    #[cfg(not(target_os = "windows"))]
    {
        drop(request);
        Err("windows_service_unsupported".into())
    }
}

pub(crate) fn setup_password(supplied: &str) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        setup::reuse_password(
            supplied,
            crate::background_share::password_for_service_setup,
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = supplied;
        Err("windows_service_unsupported".into())
    }
}
service_action!(uninstall, "--uninstall");
#[cfg(target_os = "windows")]
pub(crate) fn install_for_installer() -> Result<WindowsServiceOverview, String> {
    install::perform_action("--install")
}
service_action!(start, "--start");
pub fn start_with_fallback(
    request: crate::background_share::BackgroundShareStartRequest,
    logger: crate::app_log::AppLogger,
) -> Result<ServiceStartOutcome, String> {
    #[cfg(target_os = "windows")]
    {
        fallback::start(request, logger)
    }
    #[cfg(not(target_os = "windows"))]
    {
        drop((request, logger));
        Err("windows_service_unsupported".into())
    }
}
service_action!(stop, "--stop");
service_action!(enable_sas, "--enable-sas");
service_action!(enable_auto_start, "--enable-autostart");
service_action!(disable_auto_start, "--disable-autostart");

pub fn repair_sharing() -> Result<WindowsServiceRepairReport, String> {
    #[cfg(target_os = "windows")]
    {
        install::repair_sharing()
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("windows_service_unsupported".into())
    }
}
service_action!(
    authorize_password_management,
    "--authorize-password-management"
);

pub fn set_auto_start(enabled: bool) -> Result<WindowsServiceOverview, String> {
    if enabled {
        enable_auto_start()
    } else {
        disable_auto_start()
    }
}

pub fn configure(
    request: WindowsServiceConfigureRequest,
) -> Result<WindowsServiceOverview, String> {
    #[cfg(target_os = "windows")]
    {
        request.validate()?;
        install::configure(request)
    }
    #[cfg(not(target_os = "windows"))]
    {
        drop(request);
        Err("windows_service_unsupported".to_string())
    }
}

/// The caller supplies only a short-lived secret; endpoint changes remain administrator-only.
pub fn set_password(
    request: WindowsServicePasswordRequest,
) -> Result<WindowsServiceOverview, String> {
    #[cfg(target_os = "windows")]
    {
        request.validate()?;
        install::set_password(request)
    }
    #[cfg(not(target_os = "windows"))]
    {
        drop(request);
        Err("windows_service_unsupported".into())
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn request_secure_attention(session_id: u32) -> Result<(), String> {
    ipc::call(&ipc::Request::SecureAttention { session_id })?
        .into_result()
        .map(|_| ())
}

#[cfg(target_os = "windows")]
pub(crate) fn discover_user_agent(
    server_id: &str,
) -> Result<Option<types::ServiceAgentEndpoint>, String> {
    ipc::call(&ipc::Request::DiscoverUserAgent {
        session_id: native::current_session()?,
        server_id: server_id.to_string(),
    })?
    .into_result()
    .map(|response| response.agent_endpoint)
}

#[cfg(target_os = "windows")]
pub(crate) fn initialize_user_background() -> Result<(), String> {
    let session = native::current_session().map_err(user_token::background_error)?;
    if session == 0 {
        return Err("background_share_user_context_required".into());
    }
    let token = native::process_token(
        unsafe { windows_sys::Win32::System::Threading::GetCurrentProcess() },
        windows_sys::Win32::Security::TOKEN_QUERY,
    )
    .map_err(user_token::background_error)?;
    user_token::validate(token.0, session).map_err(user_token::background_error)?;
    native::initialize_worker_dpi_awareness().map_err(|_| "background_share_runtime_failed".into())
}

/// Headless binary entry; never starts a WebView or reads the application's .env.
pub fn run_from_args() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        install::run_from_args()
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("windows_service_unsupported".to_string())
    }
}

/// The shipped App doubles as the service installer/worker before initializing UI.
pub fn run_if_requested() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        let verb = std::env::args().nth(1);
        if verb.as_deref().is_some_and(|value| {
            matches!(
                value,
                "--service"
                    | "--status"
                    | "--install"
                    | "--install-relay"
                    | "--uninstall"
                    | "--start"
                    | "--stop"
                    | "--enable-sas"
                    | "--repair-sharing-relay"
                    | "--enable-autostart"
                    | "--disable-autostart"
                    | "--authorize-password-management"
                    | "--worker"
                    | "--user-agent"
                    | "--configure-relay"
            )
        }) {
            run_from_args()?;
            return Ok(true);
        }
    }
    Ok(false)
}

/// Preserves stable maintenance failures across the UAC helper boundary.
pub fn maintenance_exit_code(error: &str) -> i32 {
    #[cfg(target_os = "windows")]
    {
        maintenance::helper_exit_code(std::env::args().nth(1).as_deref(), error)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = error;
        1
    }
}
