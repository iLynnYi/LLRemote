use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RepairKind {
    Background,
    Service,
    Permissions,
    Sas,
    Agent,
    Elevation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RepairStatus {
    Healthy,
    Repaired,
    Deferred,
    Failed,
    NeedsAdministrator,
    RestartRequired,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowsServiceRepairItem {
    pub kind: RepairKind,
    pub status: RepairStatus,
    pub error_code: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowsServiceRepairReport {
    pub overview: WindowsServiceOverview,
    pub items: Vec<WindowsServiceRepairItem>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStartOutcome {
    pub service: Option<WindowsServiceOverview>,
    pub background: Option<crate::background_share::BackgroundShareOverview>,
    pub service_error: Option<String>,
    pub fallback_error: Option<String>,
    pub password_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowsServiceOverview {
    pub supported: bool,
    pub installed: bool,
    pub ipc_version: u32,
    pub auto_start: Option<bool>,
    pub last_service_attempt: Option<crate::startup_attempt::StartupAttempt>,
    pub status: String,
    pub worker_session_id: Option<u32>,
    pub worker_running: bool,
    pub desktop_recovering: bool,
    pub bind_address: Option<String>,
    pub port: Option<u16>,
    pub view_only: bool,
    pub credential_configured: bool,
    pub password_management_authorized: bool,
    pub password_management_supported: bool,
    pub last_error_code: Option<String>,
    pub sas_policy: String,
    pub sas_error_code: Option<String>,
    pub sas_request_count: u64,
    pub desktop: Option<String>,
    pub input_status: Option<String>,
    pub input_failures: u64,
    pub user_agent_running: bool,
    pub user_agent_port: Option<u16>,
    pub user_agent_error_code: Option<String>,
    pub user_agent_repair_supported: bool,
    pub user_agent_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<crate::local_vnc_server::LocalVncServerOverview>,
}

impl WindowsServiceOverview {
    #[cfg(any(target_os = "windows", test))]
    pub(crate) fn supports_connection_disconnect(&self) -> bool {
        self.ipc_version >= super::IPC_VERSION
    }

    pub(crate) fn unsupported() -> Self {
        Self {
            supported: false,
            installed: false,
            ipc_version: 0,
            auto_start: None,
            last_service_attempt: None,
            status: "unsupported".into(),
            worker_session_id: None,
            worker_running: false,
            desktop_recovering: false,
            bind_address: None,
            port: None,
            view_only: false,
            credential_configured: false,
            password_management_authorized: false,
            password_management_supported: false,
            last_error_code: None,
            sas_policy: "unsupported".into(),
            sas_error_code: None,
            sas_request_count: 0,
            desktop: None,
            input_status: None,
            input_failures: 0,
            user_agent_running: false,
            user_agent_port: None,
            user_agent_error_code: None,
            user_agent_repair_supported: false,
            user_agent_revision: 0,
            server: None,
        }
    }
}

// Authentication material is restricted to backend IPC and RFB discovery.
// Deliberately no Debug; never included in the overview or persisted config.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg(target_os = "windows")]
pub(crate) struct ServiceAgentEndpoint {
    pub port: u16,
    pub bootstrap_key: String,
    pub expires_at: u64,
}

#[cfg(target_os = "windows")]
impl Drop for ServiceAgentEndpoint {
    fn drop(&mut self) {
        self.bootstrap_key.zeroize();
    }
}

// Deliberately no Debug: this DTO is only a short-lived, protected IPC boundary.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowsServiceConfigureRequest {
    pub bind_address: String,
    pub port: u16,
    pub view_only: bool,
    pub password: String,
}

impl WindowsServiceConfigureRequest {
    #[cfg(any(target_os = "windows", test))]
    pub(crate) fn validate(&self) -> Result<(), String> {
        let address = self.bind_address.parse::<std::net::IpAddr>();
        if address.is_err() || self.port == 0 || self.password.len() > 8 {
            return Err("windows_service_invalid_config".into());
        }
        Ok(())
    }
}

impl Drop for WindowsServiceConfigureRequest {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

/// Secret-only delegated command; never exposes administrator configuration fields.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowsServicePasswordRequest {
    pub password: String,
}

impl WindowsServicePasswordRequest {
    #[cfg(target_os = "windows")]
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.password.is_empty() || self.password.len() > 8 {
            return Err("windows_service_invalid_config".into());
        }
        Ok(())
    }
}

impl Drop for WindowsServicePasswordRequest {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::WindowsServiceOverview;

    #[test]
    fn connection_disconnect_requires_a_versioned_service() {
        let mut value = serde_json::to_value(WindowsServiceOverview::unsupported()).unwrap();
        value.as_object_mut().unwrap().remove("ipcVersion");
        assert!(serde_json::from_value::<WindowsServiceOverview>(value).is_err());

        let mut current = WindowsServiceOverview::unsupported();
        current.ipc_version = super::super::IPC_VERSION;
        assert!(current.supports_connection_disconnect());
    }
}
