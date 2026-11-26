pub struct HostRepository {
    pub(super) connection: Mutex<Connection>,
    pub(super) credential_vault: Box<dyn CredentialVault>,
}

impl HostRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        Self::open_with_vault(path, Box::<SystemCredentialVault>::default())
    }

    pub fn open_with_vault(
        path: impl AsRef<Path>,
        credential_vault: Box<dyn CredentialVault>,
    ) -> Result<Self, String> {
        let connection = Connection::open(path).map_err(|error| error.to_string())?;
        initialize_schema(&connection)?;

        Ok(Self {
            connection: Mutex::new(connection),
            credential_vault,
        })
    }

    pub fn open_in_memory() -> Result<Self, String> {
        Self::open_in_memory_with_vault(Box::<SystemCredentialVault>::default())
    }

    pub fn open_in_memory_with_vault(
        credential_vault: Box<dyn CredentialVault>,
    ) -> Result<Self, String> {
        let connection = Connection::open_in_memory().map_err(|error| error.to_string())?;
        initialize_schema(&connection)?;

        Ok(Self {
            connection: Mutex::new(connection),
            credential_vault,
        })
    }

}
