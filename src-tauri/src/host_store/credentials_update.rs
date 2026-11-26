impl HostRepository {
    pub(super) fn resolve_updated_credential_ref(
        &self,
        host_id: &str,
        host_name: &str,
        current_credential_ref: Option<&str>,
        save_password: bool,
        password: Option<&str>,
    ) -> Result<Option<String>, String> {
        if !save_password {
            if let Some(credential_ref) = current_credential_ref {
                self.credential_vault.delete_host_password(credential_ref)?;
            }
            return Ok(None);
        }

        let password = password.unwrap_or("");
        if password.is_empty() {
            return Ok(current_credential_ref.map(ToString::to_string));
        }

        self.credential_vault
            .store_host_password(CredentialSecret {
                host_id: host_id.to_string(),
                host_name: host_name.to_string(),
                password: password.to_string(),
            })
            .map_err(|error| format!("credential vault write failed: {error}"))
            .map(Some)
    }

}
