use rusqlite::{params, Connection};
use serde_json::Value;
use std::path::Path;
use std::sync::Mutex;

pub struct TransferTaskRepository {
    connection: Mutex<Connection>,
}

impl TransferTaskRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let connection = Connection::open(path).map_err(|error| error.to_string())?;
        Self::from_connection(connection)
    }

    fn from_connection(connection: Connection) -> Result<Self, String> {
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS file_transfer_tasks (
                    position INTEGER PRIMARY KEY AUTOINCREMENT,
                    id TEXT NOT NULL UNIQUE,
                    snapshot TEXT NOT NULL
                );",
            )
            .map_err(|error| error.to_string())?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn load(&self) -> Result<Vec<Value>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut statement = connection
            .prepare("SELECT snapshot FROM file_transfer_tasks ORDER BY position")
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        rows.map(|row| {
            let raw = row.map_err(|error| error.to_string())?;
            serde_json::from_str(&raw)
                .map_err(|error| format!("file_transfer_tasks_invalid: {error}"))
        })
        .collect()
    }

    pub fn apply(&self, upserts: Vec<Value>, removes: Vec<String>) -> Result<(), String> {
        let mut connection = self.connection.lock().map_err(|error| error.to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        for task in upserts {
            let id = task
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .ok_or("file_transfer_task_id_invalid")?;
            let snapshot = serde_json::to_string(&task).map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "INSERT INTO file_transfer_tasks (id, snapshot) VALUES (?1, ?2)
                     ON CONFLICT(id) DO UPDATE SET snapshot=excluded.snapshot",
                    params![id, snapshot],
                )
                .map_err(|error| error.to_string())?;
        }
        for id in removes {
            transaction
                .execute("DELETE FROM file_transfer_tasks WHERE id = ?1", [id])
                .map_err(|error| error.to_string())?;
        }
        transaction.commit().map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_row_deltas() {
        let store =
            TransferTaskRepository::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let original = serde_json::json!({ "id": "one", "state": "queued" });
        let completed = serde_json::json!({ "id": "one", "state": "completed" });
        let second = serde_json::json!({ "id": "two" });
        store.apply(vec![original.clone()], vec![]).unwrap();
        assert_eq!(store.load().unwrap(), vec![original]);
        store
            .apply(vec![completed.clone(), second.clone()], vec![])
            .unwrap();
        assert_eq!(store.load().unwrap(), vec![completed, second.clone()]);
        store.apply(vec![], vec!["one".to_string()]).unwrap();
        assert_eq!(store.load().unwrap(), vec![second]);
    }

    #[test]
    fn failed_batch_rolls_back_every_new_task() {
        let store =
            TransferTaskRepository::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        assert!(store
            .apply(
                vec![
                    serde_json::json!({ "id": "one" }),
                    serde_json::json!({ "missing": "id" })
                ],
                vec![],
            )
            .is_err());
        assert!(store.load().unwrap().is_empty());
    }
}
