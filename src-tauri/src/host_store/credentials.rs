use super::HostRepository;

impl HostRepository {
    pub fn resolve_host_password(&self, id: &str) -> Result<Option<String>, String> {
        let host = self.get_host(id)?;
        let Some(credential_ref) = host.credential_ref else {
            return Ok(None);
        };

        self.credential_vault
            .get_host_password(&credential_ref)
            .map_err(|error| format!("credential vault read failed: {error}"))
            .map(Some)
    }
}
