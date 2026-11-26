use super::*;

impl SessionRecordingFiles {
    pub fn recover_directory(&self, directory: &Path) -> Result<Vec<RecordingJob>, String> {
        let directory = directory
            .canonicalize()
            .map_err(|_| "recording_invalid_path")?;
        for entry in fs::read_dir(&directory)
            .map_err(|_| "recording_io_failed")?
            .flatten()
        {
            if !entry.file_type().is_ok_and(|v| v.is_dir()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(id) = name
                .strip_prefix(PREFIX)
                .filter(|v| uuid::Uuid::parse_str(v).is_ok())
            else {
                continue;
            };
            if self.get(id).is_ok() {
                continue;
            }
            // One corrupt task must not hide other recordings awaiting recovery.
            let _ = self.recover_one(&directory, entry.path(), id);
        }
        self.jobs()
    }
    fn recover_one(&self, directory: &Path, root: PathBuf, id: &str) -> Result<(), String> {
        let manifest_path = root.join("manifest.json");
        if manifest_path
            .symlink_metadata()
            .map_err(|_| "recording_io_failed")?
            .file_type()
            .is_symlink()
        {
            return Err("recording_invalid_path".into());
        }
        let bytes = fs::read(manifest_path).map_err(|_| "recording_io_failed")?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("recording_metadata_limit".into());
        }
        let mut manifest: Manifest =
            serde_json::from_slice(&bytes).map_err(|_| "recording_manifest_failed")?;
        if manifest.version != 1 || manifest.id != id || manifest.segments.is_empty() {
            return Err("recording_manifest_failed".into());
        }
        validate_file_name(&manifest.file_name)?;
        if let (Some(destination), Some(expected)) = (
            manifest.destination.as_ref(),
            manifest.media_digest.as_ref(),
        ) {
            let verified = (|| {
                let mut check = File::open(destination).map_err(|_| "recording_io_failed")?;
                webm::verify_webm(&mut check, manifest.duration_ms)?;
                if hex::encode(webm::media_identity(&mut [&mut check])?.digest) != *expected {
                    return Err("recording_verification_failed".to_string());
                }
                Ok(())
            })();
            if verified.is_ok() {
                fs::remove_dir_all(root).map_err(|_| "recording_cleanup_failed")?;
                return Ok(());
            }
        }
        for (index, segment) in manifest.segments.iter().enumerate() {
            if segment.index != index + 1
                || !segment.start_ms.is_finite()
                || !segment.duration_ms.is_finite()
            {
                return Err("recording_manifest_failed".into());
            }
            let path = if segment.ready {
                segment.video(&root)
            } else {
                segment.raw(&root)
            };
            if path
                .symlink_metadata()
                .map_err(|_| "recording_io_failed")?
                .file_type()
                .is_symlink()
            {
                return Err("recording_invalid_path".into());
            }
        }
        let last = manifest.segments.last_mut().unwrap();
        if !last.ready {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(last.raw(&root))
                .map_err(|_| "recording_io_failed")?;
            let mut scanner = StreamScanner::default();
            let boundaries = scanner.scan(&mut file)?;
            let complete = scanner.complete_end();
            if complete > 0 {
                file.set_len(complete).map_err(|_| "recording_io_failed")?;
                last.raw_bytes = complete;
                last.output_bound = scanner.output_bound(complete);
                if last.duration_ms <= 0.0 {
                    let last_time = boundaries
                        .last()
                        .map(|v| v.time_ms - manifest.timeline_origin_ms.unwrap_or(0.0))
                        .unwrap_or(last.start_ms);
                    // Recover media duration from encoded blocks, including idle frames.
                    last.duration_ms = webm::media_duration_ms(&mut file)?
                        .max(last_time - last.start_ms)
                        .max(1.0);
                }
            }
        }
        manifest.duration_ms = manifest.segments.iter().map(|v| v.duration_ms).sum();
        manifest.status = "recoverable".into();
        let disk = space::disk_space(directory)?;
        let recording = RecordingFile {
            root,
            directory: directory.into(),
            volume: disk.volume,
            manifest,
            file: None,
            scanner: StreamScanner::default(),
            recent: VecDeque::new(),
            worker: false,
            progress: 0,
            error: None,
            sampled_at: Instant::now(),
            sampled_bytes: 0,
            forecast: 0,
            segment_limit: SEGMENT_BYTES,
            time_limit: SEGMENT_MS,
            finalizing_index: None,
            finalizing_written: 0,
        };
        recording.persist()?;
        self.shared.ledger.update(id, recording.demand());
        self.shared
            .files
            .lock()
            .map_err(|_| "recording_registry_failed")?
            .insert(id.into(), Arc::new(Mutex::new(recording)));
        Ok(())
    }
    pub fn release(&self, id: &str) -> Result<(), String> {
        let state = self.get(id)?;
        let mut recording = state.lock().map_err(|_| "recording_registry_failed")?;
        recording.file.take();
        let empty = recording.manifest.accepted_bytes == 0;
        if !empty {
            recording.manifest.status = "recoverable".into();
            recording.persist()?;
        }
        let root = recording.root.clone();
        drop(recording);
        self.shared
            .files
            .lock()
            .map_err(|_| "recording_registry_failed")?
            .remove(id);
        self.shared.ledger.remove(id);
        if empty {
            fs::remove_dir_all(root).map_err(|_| "recording_cleanup_failed")?;
        }
        Ok(())
    }
}
