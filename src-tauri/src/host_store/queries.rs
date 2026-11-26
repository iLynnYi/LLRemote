impl HostRepository {
    pub fn list_hosts(&self) -> Result<Vec<HostRecord>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "host store lock poisoned".to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT
                    id, name, address, port, protocol, host_group, credential_ref,
                    last_connected_at, created_at, updated_at
                FROM hosts
                ORDER BY created_at DESC, name ASC",
            )
            .map_err(|error| error.to_string())?;

        let rows = statement
            .query_map([], read_host_row)
            .map_err(|error| error.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    pub(super) fn get_host(&self, id: &str) -> Result<HostRecord, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "host store lock poisoned".to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT
                    id, name, address, port, protocol, host_group, credential_ref,
                    last_connected_at, created_at, updated_at
                FROM hosts
                WHERE id = ?1",
            )
            .map_err(|error| error.to_string())?;

        statement
            .query_row(params![id], read_host_row)
            .map_err(|error| {
                if matches!(error, rusqlite::Error::QueryReturnedNoRows) {
                    "host not found".to_string()
                } else {
                    error.to_string()
                }
            })
    }
}
