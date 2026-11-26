pub(crate) fn upsert_group(connection: &Connection, name: &str, now: &str) -> Result<(), String> {
    let name = normalize_group(name);
    connection
        .execute(
            "INSERT INTO host_groups (name, created_at, updated_at)
            VALUES (?1, ?2, ?2)
            ON CONFLICT(name) DO UPDATE SET updated_at = excluded.updated_at",
            params![name, now],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}
