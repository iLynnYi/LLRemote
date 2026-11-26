pub fn upload_agent_file_with_progress<F>(
    request: AgentFileUploadRequest,
    mut on_progress: F,
) -> Result<AgentFileUploadResult, String>
where
    F: FnMut(AgentFileUploadProgress),
{
    let source_path = PathBuf::from(&request.file_path);
    let file_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "agent_file_name_invalid".to_string())?
        .to_string();
    let metadata =
        fs::metadata(&source_path).map_err(|error| format!("agent_file_read_failed: {error}"))?;
    if !metadata.is_file() {
        return Err("agent_file_read_failed: source path is not a file".to_string());
    }
    if metadata.len() > MAX_AGENT_FILE_BYTES {
        return Err("agent_file_too_large".to_string());
    }
    let transfer_id = request
        .transfer_id
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    on_progress(AgentFileUploadProgress {
        transfer_id: transfer_id.clone(),
        file_name: file_name.clone(),
        transferred_bytes: 0,
        total_bytes: metadata.len(),
        phase: AgentFileUploadPhase::Preparing,
    });

    let create = send_agent_request(
        &request.host,
        request.port,
        serde_json::json!({
            "type": FILE_TRANSFER_CREATE,
            "sessionId": request.session_id,
            "transferId": transfer_id,
            "fileName": file_name,
            "relativePath": request.relative_path,
            "size": metadata.len(),
            "targetDirectory": request.target_directory,
            "renamePolicy": request.rename_policy,
        }),
    )?;
    if !create.ok {
        return Err(create
            .error
            .unwrap_or_else(|| "agent_file_create_failed".to_string()));
    }
    let transfer_id = create
        .transfer_id
        .ok_or_else(|| "agent_file_create_missing_transfer_id".to_string())?;
    let chunk_size = create.chunk_size.unwrap_or(DEFAULT_FILE_CHUNK_BYTES);
    let mut offset = create.offset.unwrap_or(0);
    if offset > metadata.len() || chunk_size == 0 {
        return Err("agent_file_offset_mismatch".to_string());
    }
    on_progress(AgentFileUploadProgress {
        transfer_id: transfer_id.clone(),
        file_name: file_name.clone(),
        transferred_bytes: offset,
        total_bytes: metadata.len(),
        phase: AgentFileUploadPhase::Preparing,
    });

    let mut source =
        File::open(&source_path).map_err(|error| format!("agent_file_read_failed: {error}"))?;
    let mut hasher = Sha256::new();
    // A resumed transfer must include its accepted prefix in the final digest.
    // Fresh uploads skip this loop and can send their first chunk immediately.
    let mut hashed_prefix = 0;
    let mut buffer = vec![0_u8; chunk_size];
    while hashed_prefix < offset {
        let to_read = (offset - hashed_prefix).min(buffer.len() as u64) as usize;
        let read = source
            .read(&mut buffer[..to_read])
            .map_err(|error| format!("agent_file_read_failed: {error}"))?;
        if read == 0 {
            return Err("agent_file_size_mismatch".to_string());
        }
        hasher.update(&buffer[..read]);
        hashed_prefix += read as u64;
    }

    on_progress(AgentFileUploadProgress {
        transfer_id: transfer_id.clone(),
        file_name: file_name.clone(),
        transferred_bytes: offset,
        total_bytes: metadata.len(),
        phase: AgentFileUploadPhase::Transferring,
    });
    while offset < metadata.len() {
        let to_read = (metadata.len() - offset).min(buffer.len() as u64) as usize;
        let read = source
            .read(&mut buffer[..to_read])
            .map_err(|error| format!("agent_file_read_failed: {error}"))?;
        if read == 0 {
            return Err("agent_file_size_mismatch".to_string());
        }
        // Hash the exact bytes sent, so source changes cannot make the final
        // checksum describe a different read of the same file.
        hasher.update(&buffer[..read]);
        let encoded = general_purpose::STANDARD.encode(&buffer[..read]);
        let chunk = send_agent_request(
            &request.host,
            request.port,
            serde_json::json!({
                "type": FILE_TRANSFER_CHUNK,
                "sessionId": request.session_id,
                "transferId": transfer_id,
                "offset": offset,
                "dataBase64": encoded,
            }),
        )?;
        if !chunk.ok {
            return Err(chunk
                .error
                .unwrap_or_else(|| "agent_file_chunk_failed".to_string()));
        }
        offset = chunk.offset.unwrap_or(offset + read as u64);
        on_progress(AgentFileUploadProgress {
            transfer_id: transfer_id.clone(),
            file_name: file_name.clone(),
            transferred_bytes: offset,
            total_bytes: metadata.len(),
            phase: AgentFileUploadPhase::Transferring,
        });
    }

    let expected_sha256 = hex::encode(hasher.finalize());
    on_progress(AgentFileUploadProgress {
        transfer_id: transfer_id.clone(),
        file_name: file_name.clone(),
        transferred_bytes: offset,
        total_bytes: metadata.len(),
        phase: AgentFileUploadPhase::Verifying,
    });

    let commit = send_agent_request_with_timeout(
        &request.host,
        request.port,
        serde_json::json!({
            "type": FILE_TRANSFER_COMMIT,
            "sessionId": request.session_id,
            "transferId": transfer_id,
            "size": metadata.len(),
            "sha256": expected_sha256,
        }),
        agent_file_commit_timeout(metadata.len()),
    )?;
    if !commit.ok {
        return Err(commit
            .error
            .unwrap_or_else(|| "agent_file_commit_failed".to_string()));
    }

    Ok(AgentFileUploadResult {
        file_name: commit.file_name.unwrap_or(file_name),
        remote_path: commit.remote_path.unwrap_or_default(),
        bytes_written: commit.bytes_written.unwrap_or(metadata.len()),
        sha256: commit.sha256.unwrap_or(expected_sha256),
    })
}

fn agent_file_commit_timeout(file_size_bytes: u64) -> Duration {
    let scaled_seconds = AGENT_FILE_COMMIT_MIN_TIMEOUT_SECONDS
        + (file_size_bytes / AGENT_FILE_COMMIT_BYTES_PER_TIMEOUT_SECOND);
    Duration::from_secs(scaled_seconds.clamp(
        AGENT_FILE_COMMIT_MIN_TIMEOUT_SECONDS,
        AGENT_FILE_COMMIT_MAX_TIMEOUT_SECONDS,
    ))
}
