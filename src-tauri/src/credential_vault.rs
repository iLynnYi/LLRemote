use serde::{Deserialize, Serialize};

// Keep this namespace stable across releases to preserve saved passwords.
pub(crate) const SERVICE_NAME: &str = "LLRemote Host Password";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialSecret {
    pub host_id: String,
    pub host_name: String,
    pub password: String,
}

pub trait CredentialVault: Send + Sync {
    fn store_host_password(&self, secret: CredentialSecret) -> Result<String, String>;

    fn delete_host_password(&self, credential_ref: &str) -> Result<(), String>;

    fn get_host_password(&self, credential_ref: &str) -> Result<String, String>;
}

#[derive(Debug, Default)]
pub struct SystemCredentialVault;

impl CredentialVault for SystemCredentialVault {
    fn store_host_password(&self, secret: CredentialSecret) -> Result<String, String> {
        let account = credential_account(&secret.host_id);
        let entry =
            keyring::Entry::new(SERVICE_NAME, &account).map_err(|error| error.to_string())?;
        entry
            .set_password(&secret.password)
            .map_err(|error| error.to_string())?;

        Ok(format!("system://{account}"))
    }

    fn delete_host_password(&self, credential_ref: &str) -> Result<(), String> {
        let Some(account) = credential_ref.strip_prefix("system://") else {
            return Ok(());
        };
        let entry =
            keyring::Entry::new(SERVICE_NAME, account).map_err(|error| error.to_string())?;

        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }

    fn get_host_password(&self, credential_ref: &str) -> Result<String, String> {
        let Some(account) = credential_ref.strip_prefix("system://") else {
            return Err(format!(
                "unsupported credential reference: {credential_ref}"
            ));
        };
        let entry =
            keyring::Entry::new(SERVICE_NAME, account).map_err(|error| error.to_string())?;

        match entry.get_password() {
            Ok(password) => Ok(password),
            Err(keyring::Error::NoEntry) => Err(format!(
                "no saved password found for credential reference {credential_ref}"
            )),
            Err(error) => Err(error.to_string()),
        }
    }
}

pub fn credential_account(host_id: &str) -> String {
    format!("host:{host_id}")
}
