use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSessionStartRequest {
    pub session_id: String,
    pub target: String,
    pub connection_model: SessionConnectionModel,
    pub display_source: String,
    pub input_source: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSessionUpdateRequest {
    pub session_id: String,
    pub state: Option<String>,
    pub connection_model: Option<SessionConnectionModel>,
    pub display_source: Option<String>,
    pub input_source: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSessionRfbConnectRequest {
    pub session_id: String,
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
}

impl std::fmt::Debug for RemoteSessionRfbConnectRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteSessionRfbConnectRequest")
            .field("session_id", &self.session_id)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("password", &self.password.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

impl Drop for RemoteSessionRfbConnectRequest {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        if let Some(password) = self.password.as_mut() {
            password.zeroize();
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RemoteSessionRfbInput {
    SecureAttention {
        request_id: u32,
    },
    Key {
        down: bool,
        keysym: u32,
    },
    Pointer {
        x: u16,
        y: u16,
        button_mask: u8,
    },
    Clipboard {
        text: String,
    },
    Refresh {
        incremental: bool,
    },
    SetQuality {
        quality_level: u8,
        compression_level: u8,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSessionRfbInputRequest {
    pub session_id: String,
    pub input: RemoteSessionRfbInput,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RemoteSessionRfbEvent {
    ServerRuntime {
        notice: crate::remote_desktop_core::ServerRuntimeMessage,
    },
    Performance {
        metrics: crate::remote_desktop_core::RfbPerformanceMetrics,
    },
    Bell,
    Clipboard {
        text: String,
    },
    Cursor {
        cursor: crate::remote_desktop_core::RfbCursorUpdate,
    },
    PointerPosition {
        x: u32,
        y: u32,
    },
    DesktopLayout {
        screens: Vec<crate::remote_desktop_core::RfbDesktopScreen>,
    },
    DiscoveryManifest {
        manifest: serde_json::Value,
    },
    DiscoveryError {
        error: serde_json::Value,
    },
    DiscoveryFailure {
        code: String,
    },
    Disconnected {
        reason: String,
        failure: super::RfbFailure,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBaseTransport {
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnhancedChannel {
    pub status: String,
    pub endpoint: Option<EnhancedChannelEndpoint>,
    pub authentication: String,
    pub last_error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnhancedChannelEndpoint {
    pub host_mode: String,
    pub port: u16,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerIdentity {
    pub id: Option<String>,
    pub kind: String,
    pub verification: String,
    pub vendor: Option<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub protocol_version: Option<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEndpointMatch {
    pub state: String,
    pub view_only: Option<bool>,
    pub selected_display_id: Option<String>,
    pub framebuffer_width: Option<u32>,
    pub framebuffer_height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityItem {
    pub id: String,
    pub state: String,
    pub source: String,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityMatrix {
    pub items: Vec<CapabilityItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionConnectionModel {
    pub base_transport: SessionBaseTransport,
    pub server_identity: ServerIdentity,
    pub local_endpoint_match: LocalEndpointMatch,
    pub enhanced_channel: EnhancedChannel,
    pub capabilities: CapabilityMatrix,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSurfaceAttachRequest {
    pub session_id: String,
    pub surface_id: String,
    pub owner: String,
    pub window_label: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSurfaceSnapshot {
    pub surface_id: String,
    pub owner: String,
    pub window_label: String,
    pub active: bool,
    pub surface_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSessionSnapshot {
    pub session_id: String,
    pub target: String,
    pub state: String,
    pub connection_model: SessionConnectionModel,
    pub display_source: String,
    pub input_source: String,
    pub active_surface_id: Option<String>,
    pub surfaces: Vec<SessionSurfaceSnapshot>,
}
