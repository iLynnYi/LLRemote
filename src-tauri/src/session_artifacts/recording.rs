use super::{
    space::{self, Demand, RecordingSpaceStatus, SpaceLedger},
    validate_directory, validate_file_name,
    webm::{self, StreamScanner},
    SessionArtifactWriteResult,
};
use base64::Engine;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const SEGMENT_BYTES: u64 = 1024 * 1024 * 1024;
const SEGMENT_MS: f64 = 30.0 * 60.0 * 1000.0;
const PREFIX: &str = ".llremote-recording-";
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStartRequest {
    pub directory: String,
    pub file_name: String,
}
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingFileHandle {
    pub recording_id: String,
    pub recovery_path: String,
}
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingChunkRequest {
    pub recording_id: String,
    pub offset: u64,
    pub data_base64: String,
}
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingFinishRequest {
    pub recording_id: String,
    pub duration_ms: f64,
}
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSpaceRequest {
    pub recording_id: String,
    pub pending_bytes: u64,
}
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingRetryRequest {
    pub recording_id: String,
    pub directory: Option<String>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Segment {
    index: usize,
    stream_start: u64,
    prefix: u64,
    raw_bytes: u64,
    start_ms: f64,
    duration_ms: f64,
    ready: bool,
    bytes: u64,
    output_bound: u64,
    #[serde(default)]
    media_digest: Option<String>,
}
impl Segment {
    fn raw(&self, root: &Path) -> PathBuf {
        root.join(format!("{:04}.part", self.index))
    }
    fn video(&self, root: &Path) -> PathBuf {
        root.join(format!("{:04}.webm", self.index))
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    version: u32,
    id: String,
    file_name: String,
    duration_ms: f64,
    accepted_bytes: u64,
    #[serde(default)]
    timeline_origin_ms: Option<f64>,
    segments: Vec<Segment>,
    status: String,
    destination: Option<PathBuf>,
    #[serde(default)]
    media_digest: Option<String>,
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingJob {
    pub recording_id: String,
    pub file_name: String,
    pub recovery_path: String,
    pub status: String,
    pub progress: u8,
    pub duration_ms: f64,
    pub files: Vec<SessionArtifactWriteResult>,
    pub error: Option<String>,
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSaveResult {
    pub recording_id: String,
    pub status: String,
    pub files: Vec<SessionArtifactWriteResult>,
    pub file_path: String,
    pub file_name: String,
    pub bytes_written: u64,
}
struct RecordingFile {
    root: PathBuf,
    directory: PathBuf,
    volume: String,
    manifest: Manifest,
    file: Option<File>,
    scanner: StreamScanner,
    recent: VecDeque<(u64, usize, [u8; 32])>,
    worker: bool,
    progress: u8,
    error: Option<String>,
    sampled_at: Instant,
    sampled_bytes: u64,
    forecast: u64,
    segment_limit: u64,
    time_limit: f64,
    finalizing_index: Option<usize>,
    finalizing_written: u64,
}
impl RecordingFile {
    fn current(&self) -> &Segment {
        self.manifest
            .segments
            .last()
            .expect("a recording has a segment")
    }
    fn persist(&self) -> Result<(), String> {
        write_json(&self.root.join("manifest.json"), &self.manifest)
    }
    fn demand(&self) -> Demand {
        let closing = self
            .manifest
            .segments
            .iter()
            .filter(|s| !s.ready)
            .map(|s| {
                s.output_bound
                    .saturating_sub(if self.finalizing_index == Some(s.index) {
                        self.finalizing_written
                    } else {
                        0
                    })
            })
            .fold(0_u64, u64::saturating_add);
        let total = self
            .manifest
            .segments
            .iter()
            .map(|s| if s.ready { s.bytes } else { s.output_bound })
            .fold(0_u64, u64::saturating_add);
        Demand {
            volume: self.volume.clone(),
            directory: self.directory.clone(),
            capturing: self.file.is_some(),
            pending: 0,
            closing: closing.saturating_add(if self.file.is_some() {
                self.forecast.saturating_mul(2)
            } else {
                0
            }),
            merging: 0,
            future_merge: if self.manifest.segments.len() > 1 {
                total.saturating_add(512 * self.manifest.segments.len() as u64)
            } else {
                0
            },
        }
    }
    fn files(&self) -> Vec<SessionArtifactWriteResult> {
        if self.manifest.status == "saved" {
            return self
                .manifest
                .destination
                .as_ref()
                .and_then(|path| artifact(path).ok())
                .into_iter()
                .collect();
        }
        self.manifest
            .segments
            .iter()
            .filter(|s| s.ready)
            .filter_map(|s| artifact(&s.video(&self.root)).ok())
            .collect()
    }
    fn job(&self) -> RecordingJob {
        RecordingJob {
            recording_id: self.manifest.id.clone(),
            file_name: self.manifest.file_name.clone(),
            recovery_path: self.root.to_string_lossy().into(),
            status: if self.manifest.status == "publishing" {
                "verifying".into()
            } else {
                self.manifest.status.clone()
            },
            progress: self.progress,
            duration_ms: self.manifest.duration_ms,
            files: self.files(),
            error: self.error.clone(),
        }
    }
}
#[derive(Default)]
struct Shared {
    files: Mutex<HashMap<String, Arc<Mutex<RecordingFile>>>>,
    destinations: Mutex<HashSet<PathBuf>>,
    ledger: SpaceLedger,
    index_path: Mutex<Option<PathBuf>>,
    directories: Mutex<Vec<PathBuf>>,
}
/// Registry locks protect lookup only. File encoding/remuxing never holds the
/// global registry or a capture file lock, allowing concurrent input and recording.
#[derive(Clone, Default)]
pub struct SessionRecordingFiles {
    shared: Arc<Shared>,
}
fn open_new(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "recording_file_create_failed".into())
}
fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<(), String> {
    let temporary = path.with_extension("json.tmp");
    let mut file = File::create(&temporary).map_err(|_| "recording_io_failed")?;
    file.write_all(&serde_json::to_vec(value).map_err(|_| "recording_manifest_failed")?)
        .map_err(|_| "recording_io_failed")?;
    file.sync_all().map_err(|_| "recording_io_failed")?;
    drop(file);
    // Replace atomically on Unix; Windows rename cannot replace an existing file.
    #[cfg(windows)]
    if path.exists() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let from: Vec<_> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err("recording_io_failed".into());
        }
        return Ok(());
    }
    fs::rename(temporary, path).map_err(|_| "recording_io_failed".into())
}
struct TemporaryPath(PathBuf);
impl Drop for TemporaryPath {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn publish_new(source: &Path, destination: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let source =
            CString::new(source.as_os_str().as_bytes()).map_err(|_| "recording_invalid_path")?;
        let destination = CString::new(destination.as_os_str().as_bytes())
            .map_err(|_| "recording_invalid_path")?;
        // Atomic publish without replacing another file, including on external disks.
        if unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) }
            != 0
        {
            return Err("recording_file_create_failed".into());
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let source =
            CString::new(source.as_os_str().as_bytes()).map_err(|_| "recording_invalid_path")?;
        let destination = CString::new(destination.as_os_str().as_bytes())
            .map_err(|_| "recording_invalid_path")?;
        if unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                destination.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        } != 0
        {
            return Err("recording_file_create_failed".into());
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        fs::rename(source, destination).map_err(|_| "recording_file_create_failed".into())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        fs::hard_link(source, destination)
            .and_then(|()| fs::remove_file(source))
            .map_err(|_| "recording_file_create_failed".into())
    }
}
fn artifact(path: &Path) -> Result<SessionArtifactWriteResult, String> {
    Ok(SessionArtifactWriteResult {
        file_path: path.to_string_lossy().into(),
        file_name: path
            .file_name()
            .ok_or("recording_invalid_file_name")?
            .to_string_lossy()
            .into(),
        bytes_written: path.metadata().map_err(|_| "recording_io_failed")?.len(),
    })
}
impl SessionRecordingFiles {
    pub fn configure_index(&self, path: PathBuf) -> Result<(), String> {
        *self
            .shared
            .index_path
            .lock()
            .map_err(|_| "recording_registry_failed")? = Some(path.clone());
        let dirs = if path.exists() {
            serde_json::from_slice::<Vec<PathBuf>>(
                &fs::read(&path).map_err(|_| "recording_io_failed")?,
            )
            .map_err(|_| "recording_manifest_failed")?
        } else {
            Vec::new()
        };
        *self
            .shared
            .directories
            .lock()
            .map_err(|_| "recording_registry_failed")? = dirs;
        *self
            .shared
            .index_path
            .lock()
            .map_err(|_| "recording_registry_failed")? = Some(path);
        Ok(())
    }
    pub fn recovery_directories(&self) -> Vec<PathBuf> {
        self.shared
            .directories
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default()
    }
    fn remember_directory(&self, directory: &Path) -> Result<(), String> {
        let mut dirs = self
            .shared
            .directories
            .lock()
            .map_err(|_| "recording_registry_failed")?;
        if !dirs.iter().any(|v| v == directory) {
            dirs.push(directory.into());
            if let Some(path) = self
                .shared
                .index_path
                .lock()
                .map_err(|_| "recording_registry_failed")?
                .as_ref()
            {
                write_json(path, &*dirs)?;
            }
        }
        Ok(())
    }
    fn get(&self, id: &str) -> Result<Arc<Mutex<RecordingFile>>, String> {
        self.shared
            .files
            .lock()
            .map_err(|_| "recording_registry_failed")?
            .get(id)
            .cloned()
            .ok_or("recording_not_found".into())
    }
    pub fn start(&self, request: RecordingStartRequest) -> Result<RecordingFileHandle, String> {
        let directory = validate_directory(&request.directory)?
            .canonicalize()
            .map_err(|_| "recording_invalid_path")?;
        let requested_name = validate_file_name(&request.file_name)?;
        if !requested_name.ends_with(".webm") {
            return Err("recording_invalid_file_name".into());
        }
        // Name reservation never waits for another recording's file lock. A
        // previous background validation must not delay capture or chunk lookup.
        let unique_name = {
            let mut names = self
                .shared
                .destinations
                .lock()
                .map_err(|_| "recording_registry_failed")?;
            let path = directory.join(requested_name);
            if !names.contains(&path) && path.exists() {
                return Err("recording_file_exists".into());
            }
            let name = if names.contains(&path) {
                format!(
                    "{}-{}.webm",
                    requested_name.trim_end_matches(".webm"),
                    &uuid::Uuid::new_v4().to_string()[..8]
                )
            } else {
                requested_name.to_owned()
            };
            names.insert(directory.join(&name));
            name
        };
        let name = unique_name.as_str();
        let disk = space::disk_space(&directory)?;
        if disk.available == 0 {
            return Err("recording_space_insufficient".into());
        }
        self.remember_directory(&directory)?;
        let id = uuid::Uuid::new_v4().to_string();
        let root = directory.join(format!("{PREFIX}{id}"));
        fs::create_dir(&root).map_err(|_| "recording_io_failed")?;
        let segment = Segment {
            index: 1,
            stream_start: 0,
            prefix: 0,
            raw_bytes: 0,
            start_ms: 0.0,
            duration_ms: 0.0,
            ready: false,
            bytes: 0,
            media_digest: None,
            output_bound: 512,
        };
        let recovery_path = root.to_string_lossy().into();
        let file = open_new(&segment.raw(&root))?;
        let recording = RecordingFile {
            directory,
            root,
            volume: disk.volume,
            manifest: Manifest {
                version: 1,
                id: id.clone(),
                file_name: name.into(),
                duration_ms: 0.0,
                accepted_bytes: 0,
                timeline_origin_ms: None,
                segments: vec![segment],
                status: "recording".into(),
                destination: None,
                media_digest: None,
            },
            file: Some(file),
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
        self.shared.ledger.update(&id, recording.demand());
        self.shared
            .files
            .lock()
            .map_err(|_| "recording_registry_failed")?
            .insert(id.clone(), Arc::new(Mutex::new(recording)));
        Ok(RecordingFileHandle {
            recording_id: id,
            recovery_path,
        })
    }
    pub fn append(&self, request: RecordingChunkRequest) -> Result<(), String> {
        if request.data_base64.len() > 400_000 {
            return Err("recording_chunk_limit".into());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&request.data_base64)
            .map_err(|_| "recording_invalid_chunk")?;
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        let state = self.get(&request.recording_id)?;
        let mut recording = state.lock().map_err(|_| "recording_registry_failed")?;
        if request.offset < recording.manifest.accepted_bytes {
            return if recording
                .recent
                .iter()
                .any(|v| *v == (request.offset, bytes.len(), digest))
            {
                Ok(())
            } else {
                Err("recording_chunk_out_of_order".into())
            };
        }
        if request.offset != recording.manifest.accepted_bytes || recording.file.is_none() {
            return Err("recording_chunk_out_of_order".into());
        }
        let position = recording.current().raw_bytes;
        let file = recording.file.as_mut().ok_or("recording_not_found")?;
        file.seek(SeekFrom::Start(position))
            .and_then(|_| file.write_all(&bytes))
            .map_err(|_| "recording_io_failed")?;
        recording.manifest.accepted_bytes += bytes.len() as u64;
        recording.manifest.segments.last_mut().unwrap().raw_bytes += bytes.len() as u64;
        recording
            .recent
            .push_back((request.offset, bytes.len(), digest));
        if recording.recent.len() > 128 {
            recording.recent.pop_front();
        }
        let mut scanner = std::mem::take(&mut recording.scanner);
        let scanned = scanner.scan(recording.file.as_mut().unwrap());
        let bound = scanner.output_bound(recording.current().raw_bytes);
        let prefix = scanner.prefix;
        recording.scanner = scanner;
        let boundaries = scanned?;
        if recording.manifest.timeline_origin_ms.is_none() {
            recording.manifest.timeline_origin_ms = boundaries.first().map(|b| b.time_ms);
        }
        let origin = recording.manifest.timeline_origin_ms.unwrap_or(0.0);
        let current = recording.current();
        let boundary = boundaries.into_iter().find(|b| {
            b.keyframe
                && b.start > prefix
                && b.time_ms - origin > current.start_ms
                && (b.start >= recording.segment_limit
                    || b.time_ms - origin - current.start_ms >= recording.time_limit)
        });
        recording.manifest.segments.last_mut().unwrap().output_bound = bound;
        if let Some(boundary) = boundary {
            self.rotate(
                &mut recording,
                boundary.start,
                boundary.time_ms - origin,
                prefix,
            )?;
        }
        let elapsed = recording.sampled_at.elapsed().as_secs_f64();
        if elapsed >= 1.0 {
            recording.forecast =
                ((recording.manifest.accepted_bytes - recording.sampled_bytes) as f64 / elapsed
                    * 3.0)
                    .ceil() as u64;
            recording.sampled_at = Instant::now();
            recording.sampled_bytes = recording.manifest.accepted_bytes;
        }
        self.shared
            .ledger
            .update(&request.recording_id, recording.demand());
        let finalize = recording
            .manifest
            .segments
            .iter()
            .any(|s| !s.ready && s.duration_ms > 0.0)
            && !recording.worker;
        drop(recording);
        if finalize {
            self.spawn_segments(&request.recording_id);
        }
        Ok(())
    }
    fn rotate(
        &self,
        recording: &mut RecordingFile,
        boundary: u64,
        time_ms: f64,
        prefix: u64,
    ) -> Result<(), String> {
        if prefix > 1024 * 1024 {
            return Err("recording_metadata_limit".into());
        }
        let previous = recording.current().clone();
        let next = Segment {
            index: previous.index + 1,
            stream_start: previous.stream_start + boundary - previous.prefix,
            prefix,
            raw_bytes: prefix + previous.raw_bytes - boundary,
            start_ms: time_ms,
            duration_ms: 0.0,
            ready: false,
            bytes: 0,
            media_digest: None,
            output_bound: 0,
        };
        let mut output = open_new(&next.raw(&recording.root))?;
        let input = recording.file.as_mut().unwrap();
        webm::make_streaming_header(input)?;
        input
            .seek(SeekFrom::Start(0))
            .map_err(|_| "recording_io_failed")?;
        std::io::copy(&mut input.take(prefix), &mut output).map_err(|_| "recording_io_failed")?;
        input
            .seek(SeekFrom::Start(boundary))
            .map_err(|_| "recording_io_failed")?;
        std::io::copy(&mut input.take(previous.raw_bytes - boundary), &mut output)
            .map_err(|_| "recording_io_failed")?;
        output.sync_all().map_err(|_| "recording_io_failed")?;
        let mut scanner = StreamScanner::default();
        let _ = scanner.scan(&mut output)?;
        let mut next = next;
        next.output_bound = scanner.output_bound(next.raw_bytes);
        let last = recording.manifest.segments.last_mut().unwrap();
        last.raw_bytes = boundary;
        last.duration_ms = time_ms - previous.start_ms;
        // Each completed block adds at most one cue; retain the old bound through
        // checkpointing, then refine it when the worker examines the sealed file.
        last.output_bound = last.output_bound.max(boundary);
        recording.manifest.segments.push(next);
        let previous_file = recording.file.replace(output).unwrap();
        recording.scanner = scanner;
        previous_file
            .sync_all()
            .map_err(|_| "recording_io_failed")?;
        recording.persist()?;
        previous_file
            .set_len(boundary)
            .map_err(|_| "recording_io_failed")?;
        Ok(())
    }
    fn spawn_segments(&self, id: &str) {
        let Ok(state) = self.get(id) else {
            return;
        };
        if let Ok(mut recording) = state.lock() {
            if recording.worker {
                return;
            }
            recording.worker = true;
        } else {
            return;
        }
        let this = self.clone();
        let id = id.to_owned();
        std::thread::spawn(move || {
            let result = this.finalize_segments(&id);
            if let Ok(mut recording) = state.lock() {
                recording.worker = false;
                if let Err(error) = result {
                    recording.error = Some(error);
                    recording.finalizing_index = None;
                    recording.finalizing_written = 0;
                    this.shared.ledger.update(&id, recording.demand());
                }
            }
        });
    }
    pub fn space(&self, request: RecordingSpaceRequest) -> Result<RecordingSpaceStatus, String> {
        let state = self.get(&request.recording_id)?;
        {
            let recording = state.lock().map_err(|_| "recording_registry_failed")?;
            if recording.file.is_some() && recording.error.is_some() {
                return Err("recording_segment_save_failed".into());
            }
            if let Some(file) = recording.file.as_ref() {
                file.sync_data().map_err(|_| "recording_io_failed")?;
                recording.persist()?;
            }
        }
        self.shared.ledger.status(
            &request.recording_id,
            Some(request.pending_bytes.saturating_mul(2)),
        )
    }
}

mod jobs;
mod recovery;
#[cfg(test)]
mod tests;
