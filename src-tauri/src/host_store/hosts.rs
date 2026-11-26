impl HostRepository {
    pub fn create_host(&self, request: CreateHostRequest) -> Result<HostRecord, String> {
        validate_create_password_storage(request.save_password, request.password.as_deref())?;
        let fields = normalize_host_fields(
            request.name,
            request.address,
            request.port,
            request.protocol,
            request.group,
        )?;

        let now = current_timestamp();
        let host_id = Uuid::new_v4().to_string();
        let credential_ref = if request.save_password {
            Some(
                self.credential_vault
                    .store_host_password(CredentialSecret {
                        host_id: host_id.clone(),
                        host_name: fields.name.clone(),
                        password: request.password.unwrap_or_default(),
                    })
                    .map_err(|error| format!("credential vault write failed: {error}"))?,
            )
        } else {
            None
        };
        let record = HostRecord {
            id: host_id,
            name: fields.name,
            address: fields.address,
            port: fields.port,
            protocol: fields.protocol,
            group: fields.group,
            credential_ref,
            last_connected_at: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };

        let connection = self
            .connection
            .lock()
            .map_err(|_| "host store lock poisoned".to_string())?;
        upsert_group(&connection, &record.group, &now)?;

        connection
            .execute(
                "INSERT INTO hosts (
                    id, name, address, port, protocol, host_group, credential_ref,
                    last_connected_at, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    record.id,
                    record.name,
                    record.address,
                    record.port,
                    record.protocol,
                    record.group,
                    record.credential_ref,
                    record.last_connected_at,
                    record.created_at,
                    record.updated_at,
                ],
            )
            .map_err(|error| error.to_string())?;

        Ok(record)
    }

    pub fn update_host(&self, request: UpdateHostRequest) -> Result<HostRecord, String> {
        let fields = normalize_host_fields(
            request.name,
            request.address,
            request.port,
            request.protocol,
            request.group,
        )?;
        let now = current_timestamp();
        let current = self.get_host(&request.id)?;
        let credential_ref = self.resolve_updated_credential_ref(
            &request.id,
            &fields.name,
            current.credential_ref.as_deref(),
            request.save_password,
            request.password.as_deref(),
        )?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "host store lock poisoned".to_string())?;
        upsert_group(&connection, &fields.group, &now)?;
        let changed = connection
            .execute(
                "UPDATE hosts SET
                    name = ?2,
                    address = ?3,
                    port = ?4,
                    protocol = ?5,
                    host_group = ?6,
                    credential_ref = ?7,
                    updated_at = ?8
                WHERE id = ?1",
                params![
                    request.id,
                    fields.name,
                    fields.address,
                    fields.port,
                    fields.protocol,
                    fields.group,
                    credential_ref,
                    now,
                ],
            )
            .map_err(|error| error.to_string())?;

        if changed == 0 {
            return Err("host not found".to_string());
        }

        drop(connection);
        self.get_host(&request.id)
    }

    pub fn delete_host(&self, id: &str) -> Result<(), String> {
        let existing_credential_ref = self.get_host(id).ok().and_then(|host| host.credential_ref);
        let connection = self
            .connection
            .lock()
            .map_err(|_| "host store lock poisoned".to_string())?;
        connection
            .execute("DELETE FROM hosts WHERE id = ?1", params![id])
            .map_err(|error| error.to_string())?;
        drop(connection);

        if let Some(credential_ref) = existing_credential_ref {
            self.credential_vault
                .delete_host_password(&credential_ref)
                .map_err(|error| format!("host deleted but credential cleanup failed: {error}"))?;
        }

        Ok(())
    }

}
