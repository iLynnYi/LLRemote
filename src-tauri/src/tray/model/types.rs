use crate::local_vnc_server::{
    LocalVncServerActiveConnection, LocalVncServerOverview, LocalVncServerStatus,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayStateSnapshot {
    pub status: String,
    pub icon_state: TrayIconState,
    pub local_share_running: bool,
    pub local_share_status: TrayLocalShareStatus,
    pub active_local_connection_count: u32,
    pub active_session_count: u32,
    pub failed_session_count: u32,
    pub port: Option<u16>,
    pub tooltip: String,
    pub menu_summary: Vec<String>,
    pub active_local_connections: Vec<TrayLocalConnection>,
    pub can_start_local_share: bool,
    #[serde(default)]
    pub local_share_action: TrayLocalShareAction,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayLocalConnection {
    pub id: String,
    pub label: String,
    pub detail: String,
}

impl Default for TrayStateSnapshot {
    fn default() -> Self {
        Self {
            status: "idle".to_string(),
            icon_state: TrayIconState::Idle,
            local_share_running: false,
            local_share_status: TrayLocalShareStatus::Stopped,
            active_local_connection_count: 0,
            active_session_count: 0,
            failed_session_count: 0,
            port: None,
            tooltip: "LL Remote\n○ 本机共享：未启动".to_string(),
            menu_summary: vec!["○ 本机共享：未启动".to_string()],
            active_local_connections: Vec::new(),
            can_start_local_share: false,
            local_share_action: TrayLocalShareAction::Idle,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrayIconState {
    Idle,
    Sharing,
    #[serde(rename = "sharing-active-1")]
    SharingActive1,
    #[serde(rename = "sharing-active-2")]
    SharingActive2,
    #[serde(rename = "sharing-active-3")]
    SharingActive3,
    #[serde(rename = "sharing-active-9-plus")]
    SharingActive9Plus,
    Error,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayLocalShareStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    PermissionMissing,
    PortConflict,
    Failed,
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayLocalShareAction {
    #[default]
    Idle,
    Starting,
    Stopping,
    AwaitingPermission,
    AwaitingPassword,
    Busy,
}

impl TrayLocalShareStatus {
    pub fn is_error(self) -> bool {
        matches!(
            self,
            Self::PermissionMissing | Self::PortConflict | Self::Failed
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayMenuModel {
    pub summary: Vec<String>,
    pub items: Vec<TrayMenuItemModel>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayMenuItemModel {
    pub kind: TrayMenuItemKind,
    pub label: String,
    pub payload: Option<String>,
    pub children: Vec<TrayMenuItemModel>,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayMenuItemKind {
    StartLocalShare,
    StopLocalShare,
    ActiveConnections,
    DisconnectLocalConnection,
    OpenLocalShare,
    CopyLocalShare,
    OpenMain,
    OpenSettings,
    Quit,
}

impl TrayMenuItemKind {
    pub(super) fn id(self) -> &'static str {
        match self {
            Self::StartLocalShare => "tray:start-local-share",
            Self::StopLocalShare => "tray:stop-local-share",
            Self::ActiveConnections => "tray:active-connections",
            Self::DisconnectLocalConnection => "tray:disconnect-local-connection",
            Self::OpenLocalShare => "tray:open-local-share",
            Self::CopyLocalShare => "tray:copy-local-share",
            Self::OpenMain => "tray:open-main",
            Self::OpenSettings => "tray:open-settings",
            Self::Quit => "tray:quit",
        }
    }
}
