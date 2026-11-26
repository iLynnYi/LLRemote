//! One elevated operation initializes sharing without opening its listener.
use super::WindowsServiceConfigureRequest;

pub(super) fn reuse_password(
    supplied: &str,
    read: impl FnOnce() -> Result<zeroize::Zeroizing<String>, String>,
) -> Result<String, String> {
    if !supplied.is_empty() {
        return Err("windows_service_invalid_config".into());
    }
    Ok(read()?.to_string())
}

pub(super) trait Operations {
    fn inspect(&mut self) -> Result<(bool, bool), String>;
    fn install(&mut self) -> Result<(), String>;
    fn configure(&mut self, request: WindowsServiceConfigureRequest) -> Result<(), String>;
    fn authorize(&mut self) -> Result<(), String>;
}

pub(super) fn validate(
    request: &WindowsServiceConfigureRequest,
    configured: bool,
    credential: bool,
) -> Result<(), String> {
    request.validate()?;
    if configured {
        return Err("windows_service_invalid_config".into());
    }
    if request.password.is_empty() && !credential {
        return Err("windows_service_credential_missing".into());
    }
    Ok(())
}

pub(super) fn run(
    operations: &mut impl Operations,
    request: WindowsServiceConfigureRequest,
) -> Result<(), String> {
    let (configured, credential) = operations.inspect()?;
    validate(&request, configured, credential)?;
    operations.install()?;
    operations.configure(request)?;
    operations.authorize()
}

#[cfg(test)]
#[path = "../../tests/support/windows_service_setup_tests.rs"]
mod tests;
