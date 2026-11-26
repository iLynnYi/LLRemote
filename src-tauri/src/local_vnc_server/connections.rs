use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::local_vnc_server::types::{
    LocalVncServerActiveConnection, LocalVncServerConnectionKind, LocalVncServerConnectionStatus,
};

use super::now_unix_seconds;

struct LocalVncServerConnectionEntry {
    overview: LocalVncServerActiveConnection,
    disconnect: Arc<dyn Fn() + Send + Sync>,
}

pub(crate) struct LocalVncServerConnectionRegistry {
    next_id: AtomicUsize,
    connections: Mutex<HashMap<String, LocalVncServerConnectionEntry>>,
    changed: Option<Arc<dyn Fn() + Send + Sync>>,
}

pub(crate) struct LocalVncServerConnectionGuard {
    id: String,
    connections: Arc<LocalVncServerConnectionRegistry>,
}

impl LocalVncServerConnectionGuard {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }
}

impl LocalVncServerConnectionRegistry {
    pub(crate) fn new(changed: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
        Self {
            next_id: AtomicUsize::new(1),
            connections: Mutex::new(HashMap::new()),
            changed,
        }
    }

    pub(crate) fn register(
        self: &Arc<Self>,
        kind: LocalVncServerConnectionKind,
        peer_addr: Option<SocketAddr>,
        disconnect: Arc<dyn Fn() + Send + Sync>,
    ) -> LocalVncServerConnectionGuard {
        let id = format!(
            "local-connection-{}",
            self.next_id.fetch_add(1, Ordering::SeqCst)
        );
        let ip_address = peer_addr.map(|addr| addr.ip().to_string());
        let connection = LocalVncServerActiveConnection {
            id: id.clone(),
            kind,
            device_name: None,
            platform: None,
            ip_address,
            connected_at: now_unix_seconds(),
            status: LocalVncServerConnectionStatus::Online,
        };

        if let Ok(mut connections) = self.connections.lock() {
            connections.insert(
                id.clone(),
                LocalVncServerConnectionEntry {
                    overview: connection,
                    disconnect,
                },
            );
        }
        self.emit_changed();

        LocalVncServerConnectionGuard {
            id,
            connections: Arc::clone(self),
        }
    }

    pub(super) fn list(&self) -> Vec<LocalVncServerActiveConnection> {
        let connections = self
            .connections
            .lock()
            .map(|connections| {
                connections
                    .values()
                    .map(|entry| entry.overview.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        merge_connection_devices(connections)
    }

    fn disconnect_group(&self, connection_id: &str) -> Result<(), String> {
        let disconnectors = {
            let mut connections = self
                .connections
                .lock()
                .map_err(|_| "local_vnc_server_connections_poisoned".to_string())?;
            let group_key = connections
                .values()
                .find(|entry| {
                    entry.overview.id == connection_id
                        || device_group_key(&entry.overview) == connection_id
                })
                .map(|entry| device_group_key(&entry.overview))
                .ok_or_else(|| "connection_not_found: active connection not found".to_string())?;

            let connection_ids = connections
                .iter()
                .filter(|&(_id, entry)| device_group_key(&entry.overview) == group_key)
                .map(|(id, _entry)| id.clone())
                .collect::<Vec<_>>();

            connection_ids
                .into_iter()
                .filter_map(|id| connections.remove(&id))
                .map(|entry| entry.disconnect)
                .collect::<Vec<_>>()
        };

        self.emit_changed();
        for disconnect in disconnectors {
            disconnect();
        }
        Ok(())
    }

    pub(super) fn disconnect(&self, connection_id: &str) -> Result<(), String> {
        if self
            .connections
            .lock()
            .map(|connections| {
                connections
                    .values()
                    .any(|entry| device_group_key(&entry.overview) == connection_id)
            })
            .unwrap_or(false)
        {
            return self.disconnect_group(connection_id);
        }

        let disconnect = {
            let mut connections = self
                .connections
                .lock()
                .map_err(|_| "local_vnc_server_connections_poisoned".to_string())?;
            let entry = connections
                .remove(connection_id)
                .ok_or_else(|| "connection_not_found: active connection not found".to_string())?;
            entry.disconnect
        };

        self.emit_changed();
        disconnect();
        Ok(())
    }

    pub(super) fn disconnect_all(&self) {
        let disconnectors = self
            .connections
            .lock()
            .map(|mut connections| {
                connections
                    .drain()
                    .map(|(_, entry)| entry.disconnect)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        self.emit_changed();
        for disconnect in disconnectors {
            disconnect();
        }
    }

    fn remove(&self, connection_id: &str) {
        if let Ok(mut connections) = self.connections.lock() {
            connections.remove(connection_id);
        }
        self.emit_changed();
    }

    pub(super) fn clear(&self) {
        if let Ok(mut connections) = self.connections.lock() {
            connections.clear();
        }
        self.emit_changed();
    }

    fn emit_changed(&self) {
        if let Some(changed) = self.changed.as_ref() {
            changed();
        }
    }
}

impl Drop for LocalVncServerConnectionGuard {
    fn drop(&mut self) {
        self.connections.remove(&self.id);
    }
}

fn device_group_key(connection: &LocalVncServerActiveConnection) -> String {
    connection
        .ip_address
        .as_ref()
        .map(|ip_address| format!("ip:{ip_address}"))
        .unwrap_or_else(|| connection.id.clone())
}

fn merge_connection_devices(
    connections: Vec<LocalVncServerActiveConnection>,
) -> Vec<LocalVncServerActiveConnection> {
    let mut merged: HashMap<String, LocalVncServerActiveConnection> = HashMap::new();

    for connection in connections {
        let key = device_group_key(&connection);
        merged
            .entry(key.clone())
            .and_modify(|existing| {
                if connection.connected_at < existing.connected_at {
                    existing.connected_at = connection.connected_at.clone();
                }
                if connection.status == LocalVncServerConnectionStatus::Closing {
                    existing.status = LocalVncServerConnectionStatus::Closing;
                }
            })
            .or_insert_with(|| LocalVncServerActiveConnection {
                id: key,
                ..connection
            });
    }

    let mut connections = merged.into_values().collect::<Vec<_>>();
    connections.sort_by(|a, b| {
        a.connected_at
            .cmp(&b.connected_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    connections
}
