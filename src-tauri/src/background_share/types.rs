use crate::local_vnc_server::{
    LocalVncServerOverview, LocalVncServerPermissionKind, LocalVncServerPermissionRequestResult,
    LocalVncServerViewRequest,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum BackgroundShareLaunchMode {
    Login,
    Idle,
}

impl BackgroundShareLaunchMode {
    pub fn argument(self) -> &'static str {
        match self {
            Self::Login => "--background-share",
            Self::Idle => "--background-share-idle",
        }
    }

    pub fn from_arguments(arguments: &[String]) -> Option<Self> {
        match arguments {
            [argument] if argument == Self::Login.argument() => Some(Self::Login),
            [argument] if argument == Self::Idle.argument() => Some(Self::Idle),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackgroundShareStartRequest {
    pub bind_address: String,
    pub port: u16,
    pub view_only: bool,
    pub password: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub temporary_password: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Drop for BackgroundShareStartRequest {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundShareOverview {
    pub server: LocalVncServerOverview,
    pub service_fallback: bool,
    pub temporary_share: bool,
    pub configured: bool,
    pub credential_configured: bool,
    pub auto_start: bool,
    pub startup_registration: StartupRegistration,
    pub last_login_attempt: Option<crate::startup_attempt::StartupAttempt>,
    pub process_running: bool,
    pub last_error_code: Option<String>,
    pub runtime: Option<BackgroundShareRuntime>,
    pub current_build_id: String,
    pub permission_source: BackgroundSharePermissionSource,
    pub restart_supported: bool,
    pub ipc_version: u32,
    #[cfg(target_os = "macos")]
    pub permission_identity: Option<super::permission_identity::PermissionIdentity>,
    pub signature_changed_permissions: Vec<LocalVncServerPermissionKind>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupRegistration {
    Enabled,
    Missing,
    Disabled,
    #[default]
    Unknown,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundShareRuntime {
    pub pid: u32,
    pub build_id: String,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundSharePermissionSource {
    #[default]
    Pending,
    Background,
    BackgroundCached,
}

#[derive(Serialize, Deserialize)]
#[serde(
    tag = "action",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(super) enum Request {
    Status,
    RuntimeStatus,
    SetAutoStart { enabled: bool },
    Start(BackgroundShareStartRequest),
    StartServiceFallback(BackgroundShareStartRequest),
    Stop,
    Recover,
    View(LocalVncServerViewRequest),
    Permission(LocalVncServerPermissionKind),
    Disconnect { connection_id: String },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Reply {
    pub overview: Option<BackgroundShareOverview>,
    pub error_code: Option<String>,
    pub permission: Option<LocalVncServerPermissionRequestResult>,
}

impl Reply {
    pub fn into_permission_result(self) -> Result<LocalVncServerPermissionRequestResult, String> {
        match self.error_code {
            Some(code) => Err(code),
            None => self
                .permission
                .ok_or_else(|| "background_share_invalid_reply".into()),
        }
    }
    pub fn into_result(self) -> Result<BackgroundShareOverview, String> {
        self.error_code.map_or_else(
            || {
                let overview = self.overview.ok_or("background_share_invalid_reply")?;
                super::validate_host_protocol(&overview)?;
                Ok(overview)
            },
            Err,
        )
    }
}
