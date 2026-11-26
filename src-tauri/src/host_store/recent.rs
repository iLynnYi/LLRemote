impl HostRepository {
    /// Records when a connection starts, independently of its eventual outcome.
    pub fn record_connection_attempt(&self, id: &str) -> Result<HostRecord, String> {
        let now = current_timestamp();
        let connection = self
            .connection
            .lock()
            .map_err(|_| "host store lock poisoned".to_string())?;
        let changed = connection
            .execute(
                "UPDATE hosts SET last_connected_at = ?2, updated_at = ?2 WHERE id = ?1",
                params![id, now],
            )
            .map_err(|error| error.to_string())?;

        if changed == 0 {
            return Err("host not found".to_string());
        }

        drop(connection);
        self.get_host(id)
    }
}
