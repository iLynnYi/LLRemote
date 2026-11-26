use super::*;

impl SessionRecordingFiles {
    pub(super) fn finalize_segments(&self, id: &str) -> Result<(), String> {
        let state = self.get(id)?;
        loop {
            let plan = {
                let recording = state.lock().map_err(|_| "recording_registry_failed")?;
                recording
                    .manifest
                    .segments
                    .iter()
                    .find(|s| !s.ready && s.duration_ms > 0.0)
                    .map(|s| (recording.root.clone(), s.clone()))
            };
            let Some((root, segment)) = plan else {
                return Ok(());
            };
            let raw = segment.raw(&root);
            let destination = segment.video(&root);
            let mut input = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&raw)
                .map_err(|_| "recording_io_failed")?;
            if input.metadata().map_err(|_| "recording_io_failed")?.len() < segment.raw_bytes {
                return Err("recording_invalid_webm".into());
            }
            input
                .set_len(segment.raw_bytes)
                .map_err(|_| "recording_io_failed")?;
            let identity = webm::media_identity(&mut [&mut input])?;
            if !destination.exists() {
                let status = self.shared.ledger.status(id, None)?;
                if status.should_stop {
                    return Err("recording_space_insufficient".into());
                }
                let temporary = destination.with_extension("webm.writing");
                if temporary.exists() {
                    fs::remove_file(&temporary).map_err(|_| "recording_io_failed")?;
                }
                {
                    let mut recording = state.lock().map_err(|_| "recording_registry_failed")?;
                    recording.finalizing_index = Some(segment.index);
                    recording.finalizing_written = 0;
                }
                let _cleanup = TemporaryPath(temporary.clone());
                let mut output = self.output(id, &temporary, segment.output_bound, false)?;
                let result = webm::finalize_webm(&mut input, &mut output, segment.duration_ms)
                    .and_then(|()| {
                        output
                            .file
                            .sync_all()
                            .map_err(|_| "recording_io_failed".into())
                    });
                drop(output);
                if let Err(error) = result {
                    let _ = fs::remove_file(&temporary);
                    self.refresh_demand(id)?;
                    return Err(error);
                }
                let mut check = File::open(&temporary).map_err(|_| "recording_io_failed")?;
                webm::verify_webm(&mut check, segment.duration_ms)?;
                if webm::media_identity(&mut [&mut check])? != identity {
                    return Err("recording_verification_failed".into());
                }
                drop(check);
                publish_new(&temporary, &destination)?;
            } else {
                let mut check = File::open(&destination).map_err(|_| "recording_io_failed")?;
                webm::verify_webm(&mut check, segment.duration_ms)?;
                if webm::media_identity(&mut [&mut check])? != identity {
                    return Err("recording_verification_failed".into());
                }
            }
            {
                let mut recording = state.lock().map_err(|_| "recording_registry_failed")?;
                let current = &mut recording.manifest.segments[segment.index - 1];
                current.ready = true;
                current.media_digest = Some(hex::encode(identity.digest));
                current.bytes = destination
                    .metadata()
                    .map_err(|_| "recording_io_failed")?
                    .len();
                recording.finalizing_index = None;
                recording.finalizing_written = 0;
                recording.persist()?;
                self.shared.ledger.update(id, recording.demand());
            }
            drop(input);
            fs::remove_file(raw).map_err(|_| "recording_cleanup_failed")?;
        }
    }
    pub(super) fn refresh_demand(&self, id: &str) -> Result<(), String> {
        let state = self.get(id)?;
        let recording = state.lock().map_err(|_| "recording_registry_failed")?;
        self.shared.ledger.update(id, recording.demand());
        Ok(())
    }
    fn output(
        &self,
        id: &str,
        path: &Path,
        estimated: u64,
        merging: bool,
    ) -> Result<ProgressFile, String> {
        Ok(ProgressFile {
            file: open_new(path)?,
            shared: self.shared.clone(),
            id: id.into(),
            estimated,
            position: 0,
            progress_offset: 0,
            written: 0,
            reported: 0,
            merging,
        })
    }
    pub fn finish(&self, request: RecordingFinishRequest) -> Result<RecordingSaveResult, String> {
        let id = &request.recording_id;
        let state = self.get(id)?;
        if !request.duration_ms.is_finite() || request.duration_ms <= 0.0 {
            return Err("recording_invalid_duration".into());
        }
        loop {
            let worker = state
                .lock()
                .map_err(|_| "recording_registry_failed")?
                .worker;
            if !worker {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        {
            let mut recording = state.lock().map_err(|_| "recording_registry_failed")?;
            if recording.manifest.status == "saved" {
                return self.saved_result(&recording);
            }
            if recording.worker {
                return Err("recording_job_busy".into());
            }
            recording.worker = true;
            if let Some(file) = recording.file.take() {
                if file.sync_all().is_err() {
                    recording.worker = false;
                    recording.file = Some(file);
                    return Err("recording_io_failed".into());
                }
            }
            recording.manifest.duration_ms = request.duration_ms;
            let last = recording.manifest.segments.last_mut().unwrap();
            last.duration_ms = (request.duration_ms - last.start_ms).max(1.0);
            recording.manifest.status = "saving".into();
            recording.error = None;
            if let Err(error) = recording.persist() {
                recording.worker = false;
                return Err(error);
            }
            self.shared.ledger.update(id, recording.demand());
        }
        let result = self
            .finalize_segments(id)
            .and_then(|()| self.merge(id, None));
        self.complete_job(id, &result)?;
        result
    }
    pub(super) fn complete_job(
        &self,
        id: &str,
        result: &Result<RecordingSaveResult, String>,
    ) -> Result<(), String> {
        let state = self.get(id)?;
        let mut recording = state.lock().map_err(|_| "recording_registry_failed")?;
        recording.worker = false;
        if let Err(error) = result {
            recording.finalizing_index = None;
            recording.finalizing_written = 0;
            recording.manifest.status = "failed".into();
            recording.error = Some(error.clone());
            recording.progress = 0;
            recording.persist()?;
            self.shared.ledger.update(id, recording.demand());
        }
        Ok(())
    }
    pub(super) fn saved_result(
        &self,
        recording: &RecordingFile,
    ) -> Result<RecordingSaveResult, String> {
        let files = recording.files();
        Ok(RecordingSaveResult {
            recording_id: recording.manifest.id.clone(),
            status: recording.manifest.status.clone(),
            file_path: files
                .first()
                .map(|v| v.file_path.clone())
                .unwrap_or_else(|| recording.root.to_string_lossy().into()),
            file_name: recording.manifest.file_name.clone(),
            bytes_written: files.iter().map(|v| v.bytes_written).sum(),
            files,
        })
    }
    pub(super) fn merge(
        &self,
        id: &str,
        directory: Option<PathBuf>,
    ) -> Result<RecordingSaveResult, String> {
        let state = self.get(id)?;
        let (root, manifest, target_directory, volume) = {
            let recording = state.lock().map_err(|_| "recording_registry_failed")?;
            let dir = directory.unwrap_or_else(|| recording.directory.clone());
            let disk = space::disk_space(&dir)?;
            (
                recording.root.clone(),
                recording.manifest.clone(),
                dir,
                disk.volume,
            )
        };
        let destination = target_directory.join(&manifest.file_name);
        if destination.exists() {
            if manifest.destination.as_ref() != Some(&destination)
                || manifest.media_digest.is_none()
            {
                return Err("recording_file_exists".into());
            }
            let mut check = File::open(&destination).map_err(|_| "recording_io_failed")?;
            webm::verify_webm(&mut check, manifest.duration_ms)?;
            if Some(hex::encode(webm::media_identity(&mut [&mut check])?.digest))
                != manifest.media_digest
            {
                return Err("recording_verification_failed".into());
            }
            drop(check);
            let mut recording = state.lock().map_err(|_| "recording_registry_failed")?;
            recording.manifest.status = "saved".into();
            recording.persist()?;
            self.shared.ledger.remove(id);
            let _ = fs::remove_dir_all(&root);
            return self.saved_result(&recording);
        }
        if manifest.segments.iter().any(|s| !s.ready) {
            return Err("recording_segments_pending".into());
        }
        for segment in &manifest.segments {
            let mut source = File::open(segment.video(&root)).map_err(|_| "recording_io_failed")?;
            webm::verify_webm(&mut source, segment.duration_ms)?;
            if segment.media_digest.as_deref()
                != Some(hex::encode(webm::media_identity(&mut [&mut source])?.digest).as_str())
            {
                return Err("recording_verification_failed".into());
            }
        }
        let estimate = manifest
            .segments
            .iter()
            .map(|v| v.bytes)
            .sum::<u64>()
            .saturating_add(512 * manifest.segments.len() as u64);
        // A single finalized segment only needs an atomic rename on the same volume.
        let same_volume = space::disk_space(&root)?.volume == volume;
        if manifest.segments.len() == 1 && same_volume {
            let segment = &manifest.segments[0];
            let source = segment.video(&root);
            let mut check = File::open(&source).map_err(|_| "recording_io_failed")?;
            webm::verify_webm(&mut check, manifest.duration_ms)?;
            drop(check);
            let mut check = File::open(&source).map_err(|_| "recording_io_failed")?;
            let digest = hex::encode(webm::media_identity(&mut [&mut check])?.digest);
            drop(check);
            self.prepare_publish(id, &destination, digest)?;
            publish_new(&source, &destination)?;
        } else {
            if let Err(error) =
                self.shared
                    .ledger
                    .reserve_merge(id, &volume, target_directory.clone(), estimate)
            {
                if error != "recording_merge_space_insufficient" {
                    return Err(error);
                }
                let mut recording = state.lock().map_err(|_| "recording_registry_failed")?;
                recording.manifest.status = "segmented".into();
                recording.error = Some(error);
                recording.persist()?;
                self.shared.ledger.update(id, recording.demand());
                return self.saved_result(&recording);
            }
            {
                let mut recording = state.lock().map_err(|_| "recording_registry_failed")?;
                recording.manifest.status = "merging".into();
                recording.progress = 0;
                recording.persist()?;
            }
            let paths: Vec<_> = manifest.segments.iter().map(|v| v.video(&root)).collect();
            let mut inputs: Vec<_> = paths
                .iter()
                .map(File::open)
                .collect::<Result<_, _>>()
                .map_err(|_| "recording_io_failed")?;
            let identity = webm::media_identity(&mut inputs)?;
            let temporary = target_directory.join(format!(".{}.{id}.writing", manifest.file_name));
            if temporary.exists() {
                fs::remove_file(&temporary).map_err(|_| "recording_io_failed")?;
            }
            let _cleanup = TemporaryPath(temporary.clone());
            let mut output = self.output(id, &temporary, estimate, true)?;
            let durations: Vec<_> = manifest.segments.iter().map(|v| v.duration_ms).collect();
            let result = webm::merge_webm(&mut inputs, &mut output, &durations, &mut |_| {})
                .and_then(|()| {
                    output
                        .file
                        .sync_all()
                        .map_err(|_| "recording_io_failed".into())
                });
            drop(output);
            if let Err(error) = result {
                let _ = fs::remove_file(&temporary);
                return Err(error);
            }
            {
                let mut recording = state.lock().map_err(|_| "recording_registry_failed")?;
                recording.manifest.status = "verifying".into();
                recording.progress = 100;
            }
            let mut check = File::open(&temporary).map_err(|_| "recording_io_failed")?;
            webm::verify_webm(&mut check, manifest.duration_ms)?;
            if webm::media_identity(&mut [&mut check])? != identity {
                return Err("recording_verification_failed".into());
            }
            drop(check);
            self.prepare_publish(id, &destination, hex::encode(identity.digest))?;
            publish_new(&temporary, &destination)?;
        }
        let expected = state
            .lock()
            .map_err(|_| "recording_registry_failed")?
            .manifest
            .media_digest
            .clone();
        let mut check = File::open(&destination).map_err(|_| "recording_io_failed")?;
        webm::verify_webm(&mut check, manifest.duration_ms)?;
        let digest = hex::encode(webm::media_identity(&mut [&mut check])?.digest);
        if expected.as_deref() != Some(digest.as_str()) {
            return Err("recording_verification_failed".into());
        }
        drop(check);
        {
            let mut recording = state.lock().map_err(|_| "recording_registry_failed")?;
            recording.manifest.media_digest = Some(digest);
            recording.manifest.status = "saved".into();
            recording.manifest.destination = Some(destination);
            recording.progress = 100;
            recording.error = None;
            recording.persist()?;
        }
        self.shared.ledger.remove(id);
        // The manifest has committed the verified destination before deleting any
        // independent segments. A failed cleanup remains safely repeatable.
        if fs::remove_dir_all(&root).is_err() {
            state.lock().map_err(|_| "recording_registry_failed")?.error =
                Some("recording_cleanup_failed".into());
        }
        let recording = state.lock().map_err(|_| "recording_registry_failed")?;
        self.saved_result(&recording)
    }
    pub(super) fn prepare_publish(
        &self,
        id: &str,
        destination: &Path,
        digest: String,
    ) -> Result<(), String> {
        let state = self.get(id)?;
        let mut recording = state.lock().map_err(|_| "recording_registry_failed")?;
        recording.manifest.status = "publishing".into();
        recording.manifest.destination = Some(destination.into());
        recording.manifest.media_digest = Some(digest);
        recording.persist()
    }
    /// Moves retryable queued writes as well as already sealed segments. The
    /// frontend stops capture first; accepted offsets remain unchanged on return.
    pub fn relocate_file(
        &self,
        request: RecordingRetryRequest,
    ) -> Result<RecordingFileHandle, String> {
        let directory = request.directory.ok_or("recording_invalid_path")?;
        let id = &request.recording_id;
        let state = self.get(id)?;
        let open = {
            let mut recording = state.lock().map_err(|_| "recording_registry_failed")?;
            if recording.worker {
                return Err("recording_job_busy".into());
            }
            recording.worker = true;
            recording.file.take().is_some()
        };
        let result = self.relocate(id, &directory);
        let mut recording = state.lock().map_err(|_| "recording_registry_failed")?;
        recording.worker = false;
        if open {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(recording.current().raw(&recording.root))
                .map_err(|_| "recording_io_failed")?;
            file.set_len(recording.current().raw_bytes)
                .map_err(|_| "recording_io_failed")?;
            recording.file = Some(file);
        }
        self.shared.ledger.update(id, recording.demand());
        result?;
        Ok(RecordingFileHandle {
            recording_id: id.clone(),
            recovery_path: recording.root.to_string_lossy().into(),
        })
    }

    pub fn retry(&self, request: RecordingRetryRequest) -> Result<RecordingSaveResult, String> {
        let state = self.get(&request.recording_id)?;
        {
            let mut recording = state.lock().map_err(|_| "recording_registry_failed")?;
            if recording.worker || recording.file.is_some() {
                return Err("recording_job_busy".into());
            }
            if recording.manifest.status == "saved" {
                return self.saved_result(&recording);
            }
            recording.worker = true;
            recording.error = None;
            recording.manifest.status = "saving".into();
            self.shared
                .ledger
                .update(&request.recording_id, recording.demand());
        }
        let result = (|| {
            if let Some(directory) = request.directory.as_ref() {
                self.relocate(&request.recording_id, directory)?;
            }
            self.finalize_segments(&request.recording_id)?;
            self.merge(&request.recording_id, None)
        })();
        self.complete_job(&request.recording_id, &result)?;
        result
    }
    pub(super) fn relocate(&self, id: &str, directory: &str) -> Result<(), String> {
        let directory = validate_directory(directory)?
            .canonicalize()
            .map_err(|_| "recording_invalid_path")?;
        let state = self.get(id)?;
        let (old_root, manifest, old_directory) = {
            let recording = state.lock().map_err(|_| "recording_registry_failed")?;
            (
                recording.root.clone(),
                recording.manifest.clone(),
                recording.directory.clone(),
            )
        };
        if directory == old_directory {
            return Ok(());
        }
        let disk = space::disk_space(&directory)?;
        let required = manifest
            .segments
            .iter()
            .map(|s| {
                if s.ready {
                    s.bytes
                } else {
                    s.raw_bytes.saturating_add(s.output_bound)
                }
            })
            .fold(0_u64, u64::saturating_add);
        let transfer_id = format!("{id}-relocation");
        self.shared.ledger.update(
            &transfer_id,
            Demand {
                volume: disk.volume.clone(),
                directory: directory.clone(),
                ..Default::default()
            },
        );
        let result = (|| {
            self.shared.ledger.reserve_merge(
                &transfer_id,
                &disk.volume,
                directory.clone(),
                required,
            )?;
            let root = directory.join(format!("{PREFIX}{id}"));
            self.remember_directory(&directory)?;
            fs::create_dir(&root).map_err(|_| "recording_file_create_failed")?;
            let copy_result = (|| {
                let mut copied = 0_u64;
                for segment in &manifest.segments {
                    let source = if segment.ready {
                        segment.video(&old_root)
                    } else {
                        segment.raw(&old_root)
                    };
                    let target = if segment.ready {
                        segment.video(&root)
                    } else {
                        segment.raw(&root)
                    };
                    let mut input = File::open(source).map_err(|_| "recording_io_failed")?;
                    let mut output = self.output(&transfer_id, &target, required, true)?;
                    output.progress_offset = copied;
                    copied += std::io::copy(&mut input, &mut output)
                        .map_err(|_| "recording_io_failed")?;
                    output.file.sync_all().map_err(|_| "recording_io_failed")?;
                    drop(output);
                    // Copy validation also covers unfinished raw media that cannot yet be remuxed.
                    input
                        .seek(SeekFrom::Start(0))
                        .map_err(|_| "recording_io_failed")?;
                    let mut check = File::open(target).map_err(|_| "recording_io_failed")?;
                    if file_digest(&mut input)? != file_digest(&mut check)? {
                        return Err("recording_verification_failed".into());
                    }
                }
                write_json(&root.join("manifest.json"), &manifest)
            })();
            if let Err(error) = copy_result {
                let _ = fs::remove_dir_all(root);
                return Err(error);
            }
            {
                let mut recording = state.lock().map_err(|_| "recording_registry_failed")?;
                recording.root = root;
                recording.directory = directory;
                recording.volume = disk.volume;
                self.shared.ledger.update(id, recording.demand());
            }
            // Every source byte and the new manifest have been synced and verified.
            let _ = fs::remove_dir_all(old_root);
            Ok(())
        })();
        self.shared.ledger.remove(&transfer_id);
        result
    }
    pub fn jobs(&self) -> Result<Vec<RecordingJob>, String> {
        let states: Vec<_> = self
            .shared
            .files
            .lock()
            .map_err(|_| "recording_registry_failed")?
            .values()
            .cloned()
            .collect();
        states
            .into_iter()
            .map(|v| Ok(v.lock().map_err(|_| "recording_registry_failed")?.job()))
            .collect()
    }
}

fn file_digest(input: &mut File) -> Result<[u8; 32], String> {
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = input.read(&mut buffer).map_err(|_| "recording_io_failed")?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        std::thread::yield_now();
    }
    Ok(digest.finalize().into())
}
struct ProgressFile {
    file: File,
    shared: Arc<Shared>,
    id: String,
    estimated: u64,
    position: u64,
    progress_offset: u64,
    written: u64,
    reported: u64,
    merging: bool,
}
impl Write for ProgressFile {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let count = self.file.write(bytes)?;
        self.position += count as u64;
        self.written = self.written.max(self.position);
        if self.written - self.reported >= 512 * 1024 {
            self.reported = self.written;
            if self.merging {
                self.shared.ledger.merge_progress(
                    &self.id,
                    self.estimated
                        .saturating_sub(self.progress_offset.saturating_add(self.written)),
                );
            }
            let state = self
                .shared
                .files
                .lock()
                .ok()
                .and_then(|states| states.get(&self.id).cloned());
            if let Some(state) = state {
                if let Ok(mut recording) = state.lock() {
                    recording.progress = (self
                        .progress_offset
                        .saturating_add(self.written)
                        .saturating_mul(100)
                        / self.estimated.max(1))
                    .min(99) as u8;
                    if !self.merging {
                        recording.finalizing_written = self.written;
                        self.shared.ledger.update(&self.id, recording.demand());
                    }
                }
            }
            if self.shared.ledger.has_capture() {
                std::thread::sleep(Duration::from_millis(1));
            } else {
                std::thread::yield_now();
            }
        }
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
impl Seek for ProgressFile {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.position = self.file.seek(position)?;
        Ok(self.position)
    }
}
