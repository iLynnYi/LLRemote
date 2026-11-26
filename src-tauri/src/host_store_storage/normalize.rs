pub(crate) fn normalize_host_fields(
    name: String,
    address: String,
    port: u16,
    protocol: String,
    group: String,
) -> Result<NormalizedHostFields, String> {
    let name = name.trim().to_string();
    let address = address.trim().to_string();

    if name.is_empty() {
        return Err("host name is required".to_string());
    }

    if address.is_empty() {
        return Err("host address is required".to_string());
    }

    if protocol != "vnc" {
        return Err("only vnc hosts are supported in the current MVP".to_string());
    }

    Ok(NormalizedHostFields {
        name,
        address,
        port,
        protocol,
        group: normalize_group(&group),
    })
}

pub(crate) fn validate_create_password_storage(
    save_password: bool,
    password: Option<&str>,
) -> Result<(), String> {
    if save_password && password.unwrap_or("").is_empty() {
        return Err("password was requested but no password was provided".to_string());
    }

    Ok(())
}
