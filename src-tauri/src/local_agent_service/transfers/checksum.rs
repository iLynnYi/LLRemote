fn sha256_reader(mut file: File) -> Result<String, String> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; DEFAULT_FILE_CHUNK_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("agent_file_hash_read_failed: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hex::encode(hasher.finalize()))
}
