use rusqlite::params;

use crate::host_store::types::HostGroup;
use crate::host_store_storage::{
    current_timestamp, normalize_group, read_group, read_groups, upsert_group,
};

use super::HostRepository;

impl HostRepository {
    pub fn list_groups(&self) -> Result<Vec<HostGroup>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "host store lock poisoned".to_string())?;
        read_groups(&connection)
    }

    pub fn create_group(&self, name: &str) -> Result<HostGroup, String> {
        let name = normalize_group(name);
        let now = current_timestamp();
        let connection = self
            .connection
            .lock()
            .map_err(|_| "host store lock poisoned".to_string())?;
        upsert_group(&connection, &name, &now)?;
        read_group(&connection, &name)
    }

    pub fn rename_group(&self, from: &str, to: &str) -> Result<usize, String> {
        let from = normalize_group(from);
        let to = normalize_group(to);
        let default_group = normalize_group("");

        if from == to {
            return Ok(0);
        }

        if from == default_group {
            return Err("default group cannot be renamed".to_string());
        }

        let now = current_timestamp();
        let connection = self
            .connection
            .lock()
            .map_err(|_| "host store lock poisoned".to_string())?;
        upsert_group(&connection, &to, &now)?;
        connection
            .execute("DELETE FROM host_groups WHERE name = ?1", params![from])
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "UPDATE hosts SET host_group = ?2, updated_at = ?3 WHERE host_group = ?1",
                params![from, to, now],
            )
            .map_err(|error| error.to_string())
    }

    pub fn delete_group(&self, group: &str) -> Result<usize, String> {
        let group = normalize_group(group);
        let default_group = normalize_group("");

        if group == default_group {
            return Err("default group cannot be deleted".to_string());
        }

        let now = current_timestamp();
        let connection = self
            .connection
            .lock()
            .map_err(|_| "host store lock poisoned".to_string())?;
        upsert_group(&connection, &default_group, &now)?;
        let changed = connection
            .execute(
                "UPDATE hosts SET host_group = ?2, updated_at = ?3 WHERE host_group = ?1",
                params![group, default_group, now],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute("DELETE FROM host_groups WHERE name = ?1", params![group])
            .map_err(|error| error.to_string())?;
        Ok(changed)
    }
}
