pub(crate) fn read_host_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HostRecord> {
    Ok(HostRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        address: row.get(2)?,
        port: row.get(3)?,
        protocol: row.get(4)?,
        group: row.get(5)?,
        credential_ref: row.get(6)?,
        last_connected_at: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

pub(crate) fn read_groups(connection: &Connection) -> Result<Vec<HostGroup>, String> {
    let mut statement = connection
        .prepare(
            "SELECT
                g.name,
                COUNT(hosts.id) AS host_count,
                g.created_at,
                g.updated_at
            FROM host_groups g
            LEFT JOIN hosts ON hosts.host_group = g.name
            GROUP BY g.name, g.created_at, g.updated_at
            ORDER BY
                CASE WHEN g.name = '未分组' THEN 0 ELSE 1 END,
                host_count DESC,
                g.name ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], read_group_row)
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub(crate) fn read_group(connection: &Connection, name: &str) -> Result<HostGroup, String> {
    let mut statement = connection
        .prepare(
            "SELECT
                g.name,
                COUNT(hosts.id) AS host_count,
                g.created_at,
                g.updated_at
            FROM host_groups g
            LEFT JOIN hosts ON hosts.host_group = g.name
            WHERE g.name = ?1
            GROUP BY g.name, g.created_at, g.updated_at",
        )
        .map_err(|error| error.to_string())?;

    statement
        .query_row(params![name], read_group_row)
        .map_err(|error| {
            if matches!(error, rusqlite::Error::QueryReturnedNoRows) {
                "group not found".to_string()
            } else {
                error.to_string()
            }
        })
}

fn read_group_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HostGroup> {
    let count: i64 = row.get(1)?;
    let host_count = usize::try_from(count)
        .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(1, count))?;
    Ok(HostGroup {
        name: row.get(0)?,
        host_count,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
    })
}
