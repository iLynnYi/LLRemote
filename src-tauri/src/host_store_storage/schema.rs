const HOST_SCHEMA_VERSION: u32 = 1;

pub(crate) fn initialize_schema(connection: &Connection) -> Result<(), String> {
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    let version: u32 = transaction
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if version > HOST_SCHEMA_VERSION {
        return Err("host database schema version is not supported".into());
    }
    if version == HOST_SCHEMA_VERSION {
        return transaction.commit().map_err(|error| error.to_string());
    }
    transaction
        .execute_batch(
            "CREATE TABLE hosts (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            address TEXT NOT NULL,
            port INTEGER NOT NULL,
            protocol TEXT NOT NULL,
            host_group TEXT NOT NULL,
            credential_ref TEXT,
            last_connected_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX idx_hosts_name ON hosts(name);
        CREATE INDEX idx_hosts_group ON hosts(host_group);
        CREATE TABLE host_groups (
            name TEXT PRIMARY KEY NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );",
        )
        .map_err(|error| error.to_string())?;

    let now = current_timestamp();
    upsert_group(&transaction, &normalize_group(""), &now)?;
    transaction
        .pragma_update(None, "user_version", HOST_SCHEMA_VERSION)
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())
}
