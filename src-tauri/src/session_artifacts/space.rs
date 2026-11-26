use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Clone, Debug)]
pub struct DiskSpace {
    pub volume: String,
    pub available: u64,
}

#[cfg(unix)]
pub fn disk_space(path: &Path) -> Result<DiskSpace, String> {
    use std::{
        ffi::CString,
        os::unix::{ffi::OsStrExt, fs::MetadataExt},
    };
    let path_c = CString::new(path.as_os_str().as_bytes()).map_err(|_| "recording_invalid_path")?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: path is NUL terminated and stat points to writable storage.
    if unsafe { libc::statvfs(path_c.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return Err("recording_space_unavailable".into());
    }
    let stat = unsafe { stat.assume_init() };
    let available = u128::from(stat.f_bavail) * u128::from(stat.f_frsize);
    Ok(DiskSpace {
        volume: path
            .metadata()
            .map_err(|_| "recording_space_unavailable")?
            .dev()
            .to_string(),
        available: u64::try_from(available).unwrap_or(u64::MAX),
    })
}
#[cfg(windows)]
pub fn disk_space(path: &Path) -> Result<DiskSpace, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
    };
    let path_w: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0;
    let mut mount = vec![0_u16; 32768];
    let mut volume = vec![0_u16; 128];
    unsafe {
        if GetDiskFreeSpaceExW(
            path_w.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ) == 0
            || GetVolumePathNameW(path_w.as_ptr(), mount.as_mut_ptr(), mount.len() as u32) == 0
            || GetVolumeNameForVolumeMountPointW(
                mount.as_ptr(),
                volume.as_mut_ptr(),
                volume.len() as u32,
            ) == 0
        {
            return Err("recording_space_unavailable".into());
        }
    }
    let end = volume
        .iter()
        .position(|v| *v == 0)
        .ok_or("recording_space_unavailable")?;
    Ok(DiskSpace {
        volume: String::from_utf16_lossy(&volume[..end]).to_lowercase(),
        available,
    })
}
#[cfg(not(any(unix, windows)))]
pub fn disk_space(_: &Path) -> Result<DiskSpace, String> {
    Err("recording_space_unavailable".into())
}

#[derive(Clone, Default)]
pub struct Demand {
    pub volume: String,
    pub directory: PathBuf,
    pub capturing: bool,
    pub pending: u64,
    /// Includes forecast bytes until the next sample and exact remux index overhead.
    pub closing: u64,
    pub merging: u64,
    pub future_merge: u64,
}
#[derive(Default)]
pub struct SpaceLedger {
    demands: Mutex<HashMap<String, Demand>>,
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSpaceStatus {
    pub available_bytes: u64,
    pub closing_required_bytes: u64,
    pub merge_required_bytes: u64,
    pub merge_warning: bool,
    pub should_stop: bool,
}
impl SpaceLedger {
    pub fn update(&self, id: &str, mut demand: Demand) {
        if let Ok(mut demands) = self.demands.lock() {
            if demand.capturing {
                demand.pending = demands.get(id).map(|v| v.pending).unwrap_or(0);
            }
            demands.insert(id.into(), demand);
        }
    }
    pub fn remove(&self, id: &str) {
        if let Ok(mut demands) = self.demands.lock() {
            demands.remove(id);
        }
    }
    pub fn status(&self, id: &str, pending: Option<u64>) -> Result<RecordingSpaceStatus, String> {
        let mut demands = self
            .demands
            .lock()
            .map_err(|_| "recording_registry_failed")?;
        if let Some(pending) = pending {
            demands.get_mut(id).ok_or("recording_not_found")?.pending = pending;
        }
        let own = demands.get(id).ok_or("recording_not_found")?;
        let disk = disk_space(&own.directory)?;
        Ok(calculate_space(
            demands.values().filter(|v| v.volume == disk.volume),
            disk.available,
            0,
        ))
    }
    /// The reservation and free-space decision share a lock across every recording
    /// on the volume. Bytes already written reduce the outstanding reservation.
    pub fn reserve_merge(
        &self,
        id: &str,
        volume: &str,
        directory: PathBuf,
        required: u64,
    ) -> Result<(), String> {
        let mut demands = self
            .demands
            .lock()
            .map_err(|_| "recording_registry_failed")?;
        let disk = disk_space(&directory)?;
        let others = demands
            .iter()
            .filter(|(key, value)| key.as_str() != id && value.volume == volume)
            .map(|(_, value)| {
                value
                    .pending
                    .saturating_add(value.closing)
                    .saturating_add(value.merging)
            })
            .fold(0_u64, u64::saturating_add);
        if disk.available <= others.saturating_add(required) {
            return Err("recording_merge_space_insufficient".into());
        }
        let own = demands.get_mut(id).ok_or("recording_not_found")?;
        // Merging elsewhere still accounts for outstanding closing work on the original disk.
        if own.closing != 0 {
            return Err("recording_segments_pending".into());
        }
        own.volume = volume.into();
        own.directory = directory;
        own.merging = required;
        own.future_merge = 0;
        Ok(())
    }
    pub fn merge_progress(&self, id: &str, remaining: u64) {
        if let Ok(mut demands) = self.demands.lock() {
            if let Some(own) = demands.get_mut(id) {
                own.merging = remaining;
            }
        }
    }
    pub fn has_capture(&self) -> bool {
        self.demands
            .lock()
            .is_ok_and(|v| v.values().any(|v| v.capturing))
    }
}
pub fn calculate_space<'a>(
    demands: impl Iterator<Item = &'a Demand>,
    available: u64,
    pending: u64,
) -> RecordingSpaceStatus {
    let mut closing = pending;
    let mut merging = 0_u64;
    for demand in demands {
        closing = closing
            .saturating_add(demand.pending)
            .saturating_add(demand.closing)
            .saturating_add(demand.merging);
        merging = merging.saturating_add(demand.future_merge);
    }
    let merge = closing.saturating_add(merging);
    RecordingSpaceStatus {
        available_bytes: available,
        closing_required_bytes: closing,
        merge_required_bytes: merge,
        merge_warning: available <= merge,
        should_stop: available <= closing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn warning_and_stop_include_every_recording_and_background_reservation_without_fixed_margin() {
        let demands = [
            Demand {
                pending: 20,
                closing: 100,
                future_merge: 400,
                ..Default::default()
            },
            Demand {
                pending: 30,
                closing: 200,
                merging: 300,
                future_merge: 0,
                ..Default::default()
            },
        ];
        let normal = calculate_space(demands.iter(), 2000, 0);
        assert_eq!(normal.closing_required_bytes, 650);
        assert_eq!(normal.merge_required_bytes, 1050);
        assert!(!normal.merge_warning);
        assert!(!normal.should_stop);
        let warning = calculate_space(demands.iter(), 900, 0);
        assert!(warning.merge_warning);
        assert!(!warning.should_stop);
        assert!(calculate_space(demands.iter(), 650, 0).should_stop);
        assert_eq!(
            calculate_space(std::iter::empty(), 1, 0).closing_required_bytes,
            0
        );
    }
    #[test]
    fn volume_identity_matches_across_different_directories_on_the_same_disk() {
        let directory = std::env::temp_dir();
        assert_eq!(
            disk_space(&directory).unwrap().volume,
            disk_space(directory.parent().unwrap()).unwrap().volume
        );
    }
}
