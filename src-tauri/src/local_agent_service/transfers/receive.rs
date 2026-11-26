fn create_file_transfer(
    request: AgentRequest,
    transfers: &AgentTransferMap,
    inbox_dir: &Path,
) -> Result<AgentResponse, String> {
    if let Some(id) = request.transfer_id.as_deref() {
        validate_agent_transfer_id(id)?;
    }
    let file_name = sanitize_agent_file_name(
        request
            .file_name
            .as_deref()
            .ok_or_else(|| "agent_file_name_required".to_string())?,
    )?;
    let expected_size = request
        .size
        .ok_or_else(|| "agent_file_size_required".to_string())?;
    if expected_size > MAX_AGENT_FILE_BYTES {
        return Err("agent_file_too_large".to_string());
    }
    let receive_dir = resolve_receive_directory(request.target_directory.as_deref(), inbox_dir)?;
    // Serialize admission so simultaneous creates cannot replace one ID's state.
    let mut guard = transfers
        .lock()
        .map_err(|_| "agent_transfer_state_poisoned".to_string())?;
    if let Some(transfer_id) = request.transfer_id.as_deref() {
        if let Some(transfer) = guard.get_mut(transfer_id) {
            return response_for_existing_transfer(
                transfer_id,
                transfer,
                &file_name,
                request.relative_path.as_deref(),
                expected_size,
            );
        }
    }
    let receive_dir = create_transfer_receive_directory(
        &receive_dir,
        &file_name,
        request.relative_path.as_deref(),
    )?;

    let transfer_id = request
        .transfer_id
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let final_path =
        resolve_final_receive_path(&receive_dir, &file_name, request.rename_policy.as_deref())?;
    let rename_policy = request.rename_policy.unwrap_or_else(|| "autoRename".into());
    let part_path = create_agent_part_file(&receive_dir)?;
    guard.insert(
        transfer_id.clone(),
        AgentTransferState {
            file_name,
            relative_path: request.relative_path,
            paused: false,
            committing: false,
            part_path,
            final_path,
            rename_policy,
            offset: 0,
            expected_size,
        },
    );

    Ok(AgentResponse {
        ok: true,
        error: None,
        transfer_id: Some(transfer_id),
        chunk_size: Some(DEFAULT_FILE_CHUNK_BYTES),
        offset: Some(0),
        file_name: None,
        remote_path: None,
        bytes_written: None,
        sha256: None,
        permissions: None,
        displays: None,
        secure_input: None,
        session_id: None,
        description: None,
        text: None,
        revision: None,
    })
}

fn existing_transfer_response(
    transfer_id: &str,
    file_name: &str,
    relative_path: Option<&str>,
    expected_size: u64,
    transfers: &AgentTransferMap,
) -> Result<Option<AgentResponse>, String> {
    let mut guard = transfers
        .lock()
        .map_err(|_| "agent_transfer_state_poisoned".to_string())?;
    let Some(transfer) = guard.get_mut(transfer_id) else {
        return Ok(None);
    };
    response_for_existing_transfer(
        transfer_id,
        transfer,
        file_name,
        relative_path,
        expected_size,
    )
    .map(Some)
}

fn response_for_existing_transfer(
    transfer_id: &str,
    transfer: &mut AgentTransferState,
    file_name: &str,
    relative_path: Option<&str>,
    expected_size: u64,
) -> Result<AgentResponse, String> {
    ensure_transfer_writable(transfer)?;
    if transfer.file_name != file_name
        || transfer.relative_path.as_deref() != relative_path
        || transfer.expected_size != expected_size
    {
        return Err("agent_transfer_fingerprint_mismatch".to_string());
    }
    let actual_offset = open_agent_part_file(&transfer.part_path, false)?
        .metadata()
        .map_err(|error| format!("agent_file_part_stat_failed: {error}"))?
        .len();
    if actual_offset != transfer.offset {
        return Err("agent_file_offset_mismatch".to_string());
    }
    transfer.paused = false;
    let mut response = ok_response();
    response.transfer_id = Some(transfer_id.to_string());
    response.chunk_size = Some(DEFAULT_FILE_CHUNK_BYTES);
    response.offset = Some(transfer.offset);
    Ok(response)
}

fn resume_file_transfer(
    request: AgentRequest,
    transfers: &AgentTransferMap,
) -> Result<AgentResponse, String> {
    let transfer_id = request
        .transfer_id
        .ok_or_else(|| "agent_transfer_id_required".to_string())?;
    let expected_size = request
        .size
        .ok_or_else(|| "agent_file_size_required".to_string())?;
    let file_name = sanitize_agent_file_name(
        request
            .file_name
            .as_deref()
            .ok_or_else(|| "agent_file_name_required".to_string())?,
    )?;
    existing_transfer_response(
        &transfer_id,
        &file_name,
        request.relative_path.as_deref(),
        expected_size,
        transfers,
    )?
    .ok_or_else(|| "agent_transfer_not_found".to_string())
}

fn status_file_transfer(
    request: AgentRequest,
    transfers: &AgentTransferMap,
) -> Result<AgentResponse, String> {
    let transfer_id = request
        .transfer_id
        .ok_or_else(|| "agent_transfer_id_required".to_string())?;
    let guard = transfers
        .lock()
        .map_err(|_| "agent_transfer_state_poisoned".to_string())?;
    let transfer = guard
        .get(&transfer_id)
        .ok_or_else(|| "agent_transfer_not_found".to_string())?;
    let actual_offset = open_agent_part_file(&transfer.part_path, false)?
        .metadata()
        .map_err(|error| format!("agent_file_part_stat_failed: {error}"))?
        .len();
    if actual_offset != transfer.offset {
        return Err("agent_file_offset_mismatch".to_string());
    }
    let mut response = ok_response();
    response.transfer_id = Some(transfer_id);
    response.chunk_size = Some(DEFAULT_FILE_CHUNK_BYTES);
    response.offset = Some(transfer.offset);
    response.file_name = Some(transfer.file_name.clone());
    response.bytes_written = Some(transfer.offset);
    Ok(response)
}

fn write_file_transfer_chunk(
    request: AgentRequest,
    transfers: &AgentTransferMap,
) -> Result<AgentResponse, String> {
    let transfer_id = request
        .transfer_id
        .ok_or_else(|| "agent_transfer_id_required".to_string())?;
    let offset = request
        .offset
        .ok_or_else(|| "agent_file_offset_required".to_string())?;
    let data = general_purpose::STANDARD
        .decode(request.data_base64.unwrap_or_default())
        .map_err(|error| format!("agent_file_chunk_decode_failed: {error}"))?;
    let mut guard = transfers
        .lock()
        .map_err(|_| "agent_transfer_state_poisoned".to_string())?;
    let transfer = guard
        .get_mut(&transfer_id)
        .ok_or_else(|| "agent_transfer_not_found".to_string())?;

    ensure_transfer_writable(transfer)?;

    if transfer.paused {
        return Err("agent_file_paused".to_string());
    }
    if transfer.offset != offset {
        return Err("agent_file_offset_mismatch".to_string());
    }
    if transfer.offset + data.len() as u64 > transfer.expected_size {
        return Err("agent_file_chunk_exceeds_size".to_string());
    }

    let mut file = open_agent_part_file(&transfer.part_path, true)?;
    file.write_all(&data)
        .map_err(|error| format!("agent_file_part_write_failed: {error}"))?;
    transfer.offset += data.len() as u64;

    let mut response = ok_response();
    response.offset = Some(transfer.offset);
    Ok(response)
}

fn commit_file_transfer(
    request: AgentRequest,
    transfers: &AgentTransferMap,
) -> Result<AgentResponse, String> {
    let transfer_id = request
        .transfer_id
        .ok_or_else(|| "agent_transfer_id_required".to_string())?;
    let expected_size = request
        .size
        .ok_or_else(|| "agent_file_size_required".to_string())?;
    let expected_sha256 = request
        .sha256
        .ok_or_else(|| "agent_file_sha256_required".to_string())?;
    let mut transfer = {
        let mut guard = transfers
            .lock()
            .map_err(|_| "agent_transfer_state_poisoned".to_string())?;
        let transfer = guard
            .get_mut(&transfer_id)
            .ok_or("agent_transfer_not_found")?;
        ensure_transfer_writable(transfer)?;
        if transfer.paused {
            return Err("agent_file_paused".to_string());
        }
        transfer.committing = true;
        transfer.clone()
    };

    if transfer.offset != expected_size || transfer.expected_size != expected_size {
        let _ = fs::remove_file(&transfer.part_path);
        transfers
            .lock()
            .map_err(|_| "agent_transfer_state_poisoned".to_string())?
            .remove(&transfer_id);
        return Err("agent_file_size_mismatch".to_string());
    }

    let result = (|| {
        let actual_sha256 = sha256_reader(open_agent_part_file(&transfer.part_path, false)?)?;
        if actual_sha256 != expected_sha256 {
            return Err("agent_file_checksum_mismatch".to_string());
        }
        commit_received_file(&mut transfer)?;
        let mut response = ok_response();
        response.file_name = Some(transfer.file_name.clone());
        response.remote_path = Some(transfer.final_path.to_string_lossy().to_string());
        response.bytes_written = Some(expected_size);
        response.sha256 = Some(actual_sha256);
        Ok(response)
    })();
    if result
        .as_ref()
        .is_err_and(|error| error == "agent_file_checksum_mismatch")
    {
        let _ = fs::remove_file(&transfer.part_path);
        transfers
            .lock()
            .map_err(|_| "agent_transfer_state_poisoned".to_string())?
            .remove(&transfer_id);
    } else if result.is_err() {
        // An IO or late name conflict must leave the verified upload recoverable.
        transfer.committing = false;
        transfers
            .lock()
            .map_err(|_| "agent_transfer_state_poisoned".to_string())?
            .insert(transfer_id, transfer);
    } else {
        transfers
            .lock()
            .map_err(|_| "agent_transfer_state_poisoned".to_string())?
            .remove(&transfer_id);
    }
    result
}

fn cancel_file_transfer(
    request: AgentRequest,
    transfers: &AgentTransferMap,
) -> Result<AgentResponse, String> {
    let transfer_id = request
        .transfer_id
        .ok_or_else(|| "agent_transfer_id_required".to_string())?;
    let mut guard = transfers
        .lock()
        .map_err(|_| "agent_transfer_state_poisoned".to_string())?;
    if let Some(transfer) = guard.get(&transfer_id) {
        ensure_transfer_writable(transfer)?;
    }
    if let Some(transfer) = guard.remove(&transfer_id) {
        let _ = fs::remove_file(transfer.part_path);
    }

    Ok(ok_response())
}

fn pause_file_transfer(
    request: AgentRequest,
    transfers: &AgentTransferMap,
) -> Result<AgentResponse, String> {
    let transfer_id = request
        .transfer_id
        .ok_or_else(|| "agent_transfer_id_required".to_string())?;
    let mut guard = transfers
        .lock()
        .map_err(|_| "agent_transfer_state_poisoned".to_string())?;
    let transfer = guard
        .get_mut(&transfer_id)
        .ok_or_else(|| "agent_transfer_not_found".to_string())?;
    ensure_transfer_writable(transfer)?;
    transfer.paused = true;
    let mut response = ok_response();
    response.transfer_id = Some(transfer_id);
    response.offset = Some(transfer.offset);
    response.bytes_written = Some(transfer.offset);
    Ok(response)
}
