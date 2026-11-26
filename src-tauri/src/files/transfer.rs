use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::ffi::{CStr, CString};
use std::fs;
#[cfg(target_os = "macos")]
use std::os::raw::{c_char, c_void};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTransferSource {
    pub file_path: String,
    pub relative_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTransferSourcePage {
    pub sources: Vec<FileTransferSource>,
    pub done: bool,
}

struct DirectoryFrame {
    root: PathBuf,
    entries: std::vec::IntoIter<PathBuf>,
}

pub struct FileTransferSourceScan {
    roots: std::vec::IntoIter<PathBuf>,
    directories: Vec<DirectoryFrame>,
    found_file: bool,
}

impl FileTransferSourceScan {
    pub fn new(paths: Vec<String>) -> Self {
        Self {
            roots: paths
                .into_iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>()
                .into_iter(),
            directories: Vec::new(),
            found_file: false,
        }
    }

    pub fn next_page(&mut self, limit: usize) -> Result<FileTransferSourcePage, String> {
        let mut sources = Vec::new();
        while sources.len() < limit.clamp(1, 200) {
            let (path, root) = if let Some(frame) = self.directories.last_mut() {
                if let Some(path) = frame.entries.next() {
                    (path, frame.root.clone())
                } else {
                    self.directories.pop();
                    continue;
                }
            } else if let Some(path) = self.roots.next() {
                let root = path.clone();
                (path, root)
            } else {
                break;
            };
            let metadata = fs::metadata(&path)
                .map_err(|error| format!("file_transfer_source_unreadable: {error}"))?;
            if metadata.is_dir() {
                let mut entries = fs::read_dir(&path)
                    .map_err(|error| format!("file_transfer_directory_unreadable: {error}"))?
                    .map(|entry| entry.map(|value| value.path()))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| format!("file_transfer_directory_unreadable: {error}"))?;
                entries.sort();
                self.directories.push(DirectoryFrame {
                    root,
                    entries: entries.into_iter(),
                });
            } else if metadata.is_file() {
                let relative_path = if path == root {
                    None
                } else {
                    Some(directory_relative_path(&root, &path)?)
                };
                sources.push(FileTransferSource {
                    file_path: path.to_string_lossy().to_string(),
                    relative_path,
                });
                self.found_file = true;
            } else {
                return Err("file_transfer_source_unsupported".to_string());
            }
        }
        while self
            .directories
            .last()
            .is_some_and(|frame| frame.entries.len() == 0)
        {
            self.directories.pop();
        }
        let done = self.directories.is_empty() && self.roots.len() == 0;
        if done && !self.found_file {
            return Err("file_transfer_directory_empty".to_string());
        }
        Ok(FileTransferSourcePage { sources, done })
    }
}

#[derive(Default)]
pub struct FileTransferScans {
    scans: Mutex<HashMap<String, FileTransferSourceScan>>,
}

impl FileTransferScans {
    pub fn start(&self, paths: Vec<String>) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        self.scans
            .lock()
            .map_err(|error| error.to_string())?
            .insert(id.clone(), FileTransferSourceScan::new(paths));
        Ok(id)
    }

    pub fn next(&self, id: &str, limit: usize) -> Result<FileTransferSourcePage, String> {
        let mut scans = self.scans.lock().map_err(|error| error.to_string())?;
        let scan = scans.get_mut(id).ok_or("file_transfer_scan_missing")?;
        let result = scan.next_page(limit);
        if result.as_ref().map_or(true, |page| page.done) {
            scans.remove(id);
        }
        result
    }

    pub fn cancel(&self, id: &str) -> Result<(), String> {
        self.scans
            .lock()
            .map_err(|error| error.to_string())?
            .remove(id);
        Ok(())
    }
}

pub fn select_file_transfer_sources() -> Result<Vec<String>, String> {
    select_file_transfer_sources_platform()
}

#[cfg(target_os = "macos")]
fn select_file_transfer_sources_platform() -> Result<Vec<String>, String> {
    unsafe { macos_select_file_transfer_sources() }
}

#[cfg(not(target_os = "macos"))]
fn select_file_transfer_sources_platform() -> Result<Vec<String>, String> {
    Err("file_transfer_source_dialog_unsupported".to_string())
}

fn directory_relative_path(root: &Path, file_path: &Path) -> Result<String, String> {
    let root_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "file_transfer_directory_name_invalid".to_string())?;
    let inner = file_path
        .strip_prefix(root)
        .map_err(|_| "file_transfer_relative_path_invalid".to_string())?;
    let mut parts = vec![root_name.to_string()];

    for component in inner.components() {
        if let Component::Normal(value) = component {
            parts.push(value.to_string_lossy().to_string());
        }
    }

    Ok(parts.join("/"))
}

#[cfg(target_os = "macos")]
type ObjcId = *mut c_void;
#[cfg(target_os = "macos")]
type ObjcSel = *mut c_void;
#[cfg(target_os = "macos")]
type ObjcBool = i8;

#[cfg(target_os = "macos")]
#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const c_char) -> ObjcId;
    fn sel_registerName(name: *const c_char) -> ObjcSel;
    fn objc_msgSend();
}

#[cfg(target_os = "macos")]
#[link(name = "AppKit", kind = "framework")]
extern "C" {}

#[cfg(target_os = "macos")]
unsafe fn macos_select_file_transfer_sources() -> Result<Vec<String>, String> {
    let pool = msg_id(class("NSAutoreleasePool")?, selector("new")?);
    let panel = msg_id(class("NSOpenPanel")?, selector("openPanel")?);
    if panel.is_null() {
        return Err("file_transfer_source_dialog_unavailable".to_string());
    }

    msg_void_bool(panel, selector("setCanChooseFiles:")?, true);
    msg_void_bool(panel, selector("setCanChooseDirectories:")?, true);
    msg_void_bool(panel, selector("setAllowsMultipleSelection:")?, true);
    msg_void_bool(panel, selector("setResolvesAliases:")?, true);
    msg_void_bool(
        panel,
        selector("setTreatsFilePackagesAsDirectories:")?,
        true,
    );

    let response = msg_isize(panel, selector("runModal")?);
    if response != 1 {
        msg_void(pool, selector("drain")?);
        return Ok(Vec::new());
    }

    let urls = msg_id(panel, selector("URLs")?);
    let count = msg_usize(urls, selector("count")?);
    let object_at_index = selector("objectAtIndex:")?;
    let path_selector = selector("path")?;
    let utf8_selector = selector("UTF8String")?;
    let mut paths = Vec::new();

    for index in 0..count {
        let url = msg_id_usize(urls, object_at_index, index);
        let path = msg_id(url, path_selector);
        let raw = msg_ptr(path, utf8_selector);
        if raw.is_null() {
            continue;
        }
        paths.push(CStr::from_ptr(raw).to_string_lossy().to_string());
    }

    msg_void(pool, selector("drain")?);
    Ok(paths)
}

#[cfg(target_os = "macos")]
unsafe fn class(name: &str) -> Result<ObjcId, String> {
    let class_name = CString::new(name).map_err(|_| "objc_class_name_invalid".to_string())?;
    let class = objc_getClass(class_name.as_ptr());
    if class.is_null() {
        Err(format!("objc_class_missing: {name}"))
    } else {
        Ok(class)
    }
}

#[cfg(target_os = "macos")]
fn selector(name: &str) -> Result<ObjcSel, String> {
    let selector_name = CString::new(name).map_err(|_| "objc_selector_name_invalid".to_string())?;
    let selector = unsafe { sel_registerName(selector_name.as_ptr()) };
    if selector.is_null() {
        Err(format!("objc_selector_missing: {name}"))
    } else {
        Ok(selector)
    }
}

#[cfg(target_os = "macos")]
unsafe fn msg_id(receiver: ObjcId, selector: ObjcSel) -> ObjcId {
    let send: extern "C" fn(ObjcId, ObjcSel) -> ObjcId =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector)
}

#[cfg(target_os = "macos")]
unsafe fn msg_id_usize(receiver: ObjcId, selector: ObjcSel, value: usize) -> ObjcId {
    let send: extern "C" fn(ObjcId, ObjcSel, usize) -> ObjcId =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, value)
}

#[cfg(target_os = "macos")]
unsafe fn msg_void(receiver: ObjcId, selector: ObjcSel) {
    let send: extern "C" fn(ObjcId, ObjcSel) = std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector)
}

#[cfg(target_os = "macos")]
unsafe fn msg_void_bool(receiver: ObjcId, selector: ObjcSel, value: bool) {
    let send: extern "C" fn(ObjcId, ObjcSel, ObjcBool) =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, if value { 1 } else { 0 })
}

#[cfg(target_os = "macos")]
unsafe fn msg_isize(receiver: ObjcId, selector: ObjcSel) -> isize {
    let send: extern "C" fn(ObjcId, ObjcSel) -> isize =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector)
}

#[cfg(target_os = "macos")]
unsafe fn msg_usize(receiver: ObjcId, selector: ObjcSel) -> usize {
    let send: extern "C" fn(ObjcId, ObjcSel) -> usize =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector)
}

#[cfg(target_os = "macos")]
unsafe fn msg_ptr(receiver: ObjcId, selector: ObjcSel) -> *const c_char {
    let send: extern "C" fn(ObjcId, ObjcSel) -> *const c_char =
        std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector)
}
