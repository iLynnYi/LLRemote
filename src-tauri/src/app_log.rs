use chrono::{DateTime, Local, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, SyncSender};
use std::thread;

const DEFAULT_MAX_FILE_SIZE_MB: u64 = 10;
const DEFAULT_MAX_FILES: usize = 5;
const DEFAULT_RETENTION_DAYS: u64 = 14;
const NORMAL_QUEUE_CAPACITY: usize = 2_048;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    fn parse(value: Option<String>) -> Self {
        match value.as_deref().map(str::to_ascii_lowercase).as_deref() {
            Some("error") => Self::Error,
            Some("warn") => Self::Warn,
            Some("debug") => Self::Debug,
            Some("trace") => Self::Trace,
            _ => Self::Info,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct FrontendLogEvent {
    pub level: LogLevel,
    pub component: String,
    pub event: String,
    pub message: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default, alias = "attemptId")]
    pub attempt_id: Option<String>,
    #[serde(default, alias = "sessionId")]
    pub session_id: Option<String>,
}

#[derive(Clone)]
pub struct AppLogger {
    sender: SyncSender<LogRecord>,
    level: LogLevel,
    directory: PathBuf,
}

#[derive(Clone)]
struct LogRecord {
    level: LogLevel,
    value: Value,
}

impl AppLogger {
    pub fn initialize(log_dir: PathBuf) -> Result<Self, String> {
        let level = LogLevel::parse(env::var("VNC_LOG_LEVEL").ok());
        let max_file_size = parse_env_u64("VNC_LOG_MAX_FILE_SIZE_MB", DEFAULT_MAX_FILE_SIZE_MB)
            .max(1)
            * 1024
            * 1024;
        let max_files = parse_env_usize("VNC_LOG_MAX_FILES", DEFAULT_MAX_FILES);
        let retention_days = parse_env_u64("VNC_LOG_RETENTION_DAYS", DEFAULT_RETENTION_DAYS);
        let terminal_mirror =
            cfg!(debug_assertions) && env::var("VNC_LOG_STDERR").ok().as_deref() == Some("true");
        let configured_dir = env::var("VNC_LOG_DIR")
            .ok()
            .filter(|value| Path::new(value).is_absolute())
            .map(PathBuf::from);
        let (directory, configured_directory_unavailable) = match configured_dir {
            Some(directory) if fs::create_dir_all(&directory).is_ok() => (directory, false),
            Some(_) => {
                fs::create_dir_all(&log_dir)
                    .map_err(|error| format!("create fallback log directory: {error}"))?;
                (log_dir, true)
            }
            None => {
                fs::create_dir_all(&log_dir)
                    .map_err(|error| format!("create log directory: {error}"))?;
                (log_dir, false)
            }
        };
        cleanup_archives(&directory, retention_days, max_files)?;
        let (sender, receiver) = mpsc::sync_channel(NORMAL_QUEUE_CAPACITY);
        let worker_dir = directory.clone();
        thread::Builder::new()
            .name("vnc-file-logger".into())
            .spawn(move || {
                let mut writer =
                    LogWriter::new(worker_dir, max_file_size, max_files, retention_days);
                while let Ok(record) = receiver.recv() {
                    writer.write(record, terminal_mirror);
                }
            })
            .map_err(|error| format!("start log writer: {error}"))?;
        let logger = Self {
            sender,
            level,
            directory,
        };
        logger.info(
            "logging",
            "logging_configured",
            "Local file logging configured.",
            json!({
                "effective_level": level, "terminal_mirror": terminal_mirror,
                "max_file_size_bytes": max_file_size, "max_archives": max_files,
                "retention_days": retention_days
            }),
        );
        if configured_directory_unavailable {
            logger.submit(
                LogLevel::Warn,
                "logging",
                "configured_log_directory_unavailable",
                "Configured VNC_LOG_DIR could not be used; the system application log directory is active.",
                json!({}),
            );
        }
        Ok(logger)
    }

    pub fn accepts(&self, level: LogLevel) -> bool {
        level <= self.level
    }
    pub fn open_log_directory(&self) -> Result<(), String> {
        let directory = self
            .directory
            .to_str()
            .ok_or_else(|| "log directory path is not valid UTF-8".to_string())?;
        tauri_plugin_opener::open_path(directory, None::<&str>)
            .map_err(|error| format!("open log directory: {error}"))
    }
    pub fn submit(
        &self,
        level: LogLevel,
        component: &str,
        event: &str,
        message: &str,
        extra: Value,
    ) {
        if !self.accepts(level) {
            return;
        }
        let value = json!({ "ts": Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true), "level": level,
            "component": limit(component, 80), "event": limit(event, 100), "message": redact(message), "context": extra });
        let _ = self.sender.try_send(LogRecord { level, value });
    }
    pub fn info(&self, component: &str, event: &str, message: &str, extra: Value) {
        self.submit(LogLevel::Info, component, event, message, extra);
    }
    pub fn frontend_batch(&self, events: Vec<FrontendLogEvent>) {
        for event in events.into_iter().take(20) {
            self.submit(event.level, &event.component, &event.event, &event.message, json!({
                "source": event.source.map(|value| limit(&value, 80)),
                "summary": event.summary.map(|value| redact(&value)),
                "target": event.target.map(|value| limit(&value, 255)), "attempt_id": event.attempt_id.map(|value| limit(&value, 100)),
                "session_id": event.session_id.map(|value| limit(&value, 100)), "origin": "frontend"
            }));
        }
    }
}

struct LogWriter {
    directory: PathBuf,
    max_size: u64,
    max_files: usize,
    retention_days: u64,
    file: Option<File>,
}
impl LogWriter {
    fn new(directory: PathBuf, max_size: u64, max_files: usize, retention_days: u64) -> Self {
        Self {
            directory,
            max_size,
            max_files,
            retention_days,
            file: None,
        }
    }
    fn write(&mut self, record: LogRecord, terminal_mirror: bool) {
        let line = match serde_json::to_string(&record.value) {
            Ok(line) => line,
            Err(_) => return,
        };
        if terminal_mirror {
            eprintln!("{}", format_terminal_line(&record));
        }
        if self.rotate_if_needed(line.len() as u64 + 1).is_err() {
            return;
        }
        if let Some(file) = self.file.as_mut() {
            let _ = writeln!(file, "{line}");
            let _ = file.flush();
        }
    }
    fn rotate_if_needed(&mut self, incoming: u64) -> Result<(), String> {
        let active = self.directory.join("LLRemote.log");
        let rotate = active
            .metadata()
            .map(|meta| meta.len() + incoming > self.max_size)
            .unwrap_or(false);
        if rotate {
            self.file.take();
            let archive = self.directory.join(format!(
                "LLRemote.{}.log",
                Utc::now().format("%Y%m%dT%H%M%S")
            ));
            fs::rename(&active, archive).map_err(|error| error.to_string())?;
            cleanup_archives(&self.directory, self.retention_days, self.max_files)?;
        }
        if self.file.is_none() {
            self.file = Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(active)
                    .map_err(|error| error.to_string())?,
            );
        }
        Ok(())
    }
}

fn format_terminal_line(record: &LogRecord) -> String {
    let time = record.value["ts"]
        .as_str()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| {
            value
                .with_timezone(&Local)
                .format("%H:%M:%S%.3f")
                .to_string()
        })
        .unwrap_or_else(|| "--:--:--".into());
    let level = match record.level {
        LogLevel::Error => "ERROR",
        LogLevel::Warn => "WARN",
        LogLevel::Info => "INFO",
        LogLevel::Debug => "DEBUG",
        LogLevel::Trace => "TRACE",
    };
    let context = &record.value["context"];
    let source = context["source"]
        .as_str()
        .or_else(|| record.value["component"].as_str())
        .unwrap_or("app");
    let target = context["target"]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(|value| format!(" {value}"))
        .unwrap_or_default();
    let message = context["summary"]
        .as_str()
        .or_else(|| record.value["message"].as_str())
        .unwrap_or("")
        .replace(['\n', '\r'], " ");
    format!("[{time}] {level:<5} [{source}{target}] {message}")
}

fn cleanup_archives(directory: &Path, retention_days: u64, max_files: usize) -> Result<(), String> {
    let now = std::time::SystemTime::now();
    let mut archives = fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_name().to_string_lossy().starts_with("LLRemote.")
                && entry.file_name().to_string_lossy().ends_with(".log")
        })
        .collect::<Vec<_>>();
    archives.sort_by_key(|entry| entry.metadata().and_then(|meta| meta.modified()).ok());
    for entry in &archives {
        let expired = retention_days > 0
            && entry
                .metadata()
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .map(|age| age.as_secs() > retention_days * 86_400)
                .unwrap_or(false);
        if expired {
            let _ = fs::remove_file(entry.path());
        }
    }
    archives.retain(|entry| entry.path().exists());
    if archives.len() > max_files {
        for entry in &archives[..archives.len() - max_files] {
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok(())
}
fn parse_env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}
fn parse_env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}
fn limit(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}
fn redact(value: &str) -> String {
    let value = limit(value, 2_048);
    let lower = value.to_ascii_lowercase();
    if ["password=", "token=", "authorization:"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        "[redacted sensitive diagnostic detail]".into()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_line_surfaces_level_target_and_message_without_json() {
        let record = LogRecord {
            level: LogLevel::Warn,
            value: json!({
                "ts": "2026-11-01T10:40:24.888Z",
                "component": "frontend.conversation",
                "message": "连接问题：目标不可达\n请检查网络",
                "context": { "source": "会话", "target": "192.168.85.128:5904", "session_id": "session-1" }
            }),
        };
        let line = format_terminal_line(&record);
        assert!(line.contains("WARN  [会话 192.168.85.128:5904] 连接问题：目标不可达 请检查网络"));
        assert!(!line.contains("session_id"));
        assert!(!line.contains("\n"));
    }

    #[test]
    fn terminal_line_prefers_the_user_summary_and_keeps_raw_detail_in_the_file_record() {
        let record = LogRecord {
            level: LogLevel::Info,
            value: json!({
                "ts": "2026-11-01T10:40:24.888Z",
                "component": "frontend.conversation",
                "message": "Native RFB Core transport ready; transport=native:internal-id",
                "context": { "source": "会话", "target": "192.168.85.128:5904", "summary": "已建立 VNC 连接" }
            }),
        };
        let line = format_terminal_line(&record);
        assert!(line.contains("INFO  [会话 192.168.85.128:5904] 已建立 VNC 连接"));
        assert!(!line.contains("Native RFB Core"));
        assert!(record.value["message"]
            .as_str()
            .unwrap()
            .contains("transport=native:internal-id"));
    }
}
