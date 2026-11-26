//! Read-only network connection and per-profile Windows Firewall status.
use serde::{Deserialize, Serialize};

#[cfg(target_os = "windows")]
mod native;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FirewallNetworkKind {
    Domain,
    Private,
    Public,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirewallNetwork {
    pub name: FirewallNetworkKind,
    pub connected: Option<bool>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirewallOverview {
    pub profiles: Vec<FirewallNetwork>,
}

pub fn inspect() -> Result<FirewallOverview, String> {
    #[cfg(target_os = "windows")]
    return native::inspect();
    #[cfg(not(target_os = "windows"))]
    Err("windows_firewall_unsupported".into())
}

pub fn open_settings() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    return native::open_settings();
    #[cfg(not(target_os = "windows"))]
    Err("windows_firewall_unsupported".into())
}
