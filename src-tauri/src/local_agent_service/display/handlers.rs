fn diagnose_permissions_response(inbox_dir: &Path) -> Result<AgentResponse, String> {
    let mut response = ok_response();
    response.permissions = Some(agent_permission_diagnostics(inbox_dir));
    Ok(response)
}

fn enumerate_displays_response() -> Result<AgentResponse, String> {
    let mut response = ok_response();
    response.displays = Some(agent_display_inventory());
    Ok(response)
}

fn select_display_response(
    request: AgentRequest,
    context: AgentRuntimeContext,
) -> Result<AgentResponse, String> {
    let display_id = request
        .display_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "agent_display_id_required".to_string())?;
    let displays = list_local_vnc_server_displays();
    if !displays.iter().any(|display| display.id == display_id) {
        return Err("agent_display_not_found".to_string());
    }

    let next_display =
        shared_display_for_agent_request_from_inventory(displays, Some(display_id), None);
    let mut shared = context
        .shared_display
        .write()
        .map_err(|_| "agent_display_state_poisoned".to_string())?;
    shared.replace_with(next_display).map_err(str::to_string)?;
    Ok(ok_response())
}

fn wake_display_response() -> Result<AgentResponse, String> {
    wake_remote_display()?;
    Ok(ok_response())
}

fn remote_desktop_control_response(request: AgentRequest) -> Result<AgentResponse, String> {
    let action_id = request
        .action_id
        .ok_or_else(|| "remote_desktop_action_required".to_string())?;
    control_remote_desktop(&action_id)?;
    Ok(ok_response())
}

fn recover_secure_input_response(request: AgentRequest) -> Result<AgentResponse, String> {
    let mut response = ok_response();
    let reason = request
        .reason
        .unwrap_or_else(|| "input_injection_lost".to_string());
    let mut result = unsupported_secure_input_recovery();
    result.message = format!(
        "Secure input recovery for {reason} requires a Windows service-mode Agent or a VNC Server private capability."
    );
    response.secure_input = Some(result);
    Ok(response)
}

fn unsupported_secure_input_recovery() -> AgentSecureInputRecoveryResult {
    AgentSecureInputRecoveryResult {
        target: String::new(),
        checked_at: String::new(),
        status: "unsupported".to_string(),
        action: "none".to_string(),
        message: "Secure input recovery requires a Windows service-mode Agent.".to_string(),
    }
}
