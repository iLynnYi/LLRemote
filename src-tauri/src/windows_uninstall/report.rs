//! Core removal succeeds independently of optional cleanup. Never hide residues.
use serde::{Deserialize, Serialize};
use std::{io, path::Path};

#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CleanupReport {
    pub warnings: Vec<Residue>,
    pub reboot: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Residue {
    pub code: i32,
    pub path: String,
}

/// Optional secret erasure must not block removal of a stopped service.
pub(crate) fn remove_service_core(
    reset: Result<(), String>,
    stop: impl FnOnce() -> Result<(), String>,
    remove: impl FnOnce() -> Result<(), String>,
    credential_location: &Path,
) -> Result<CleanupReport, String> {
    stop()?;
    remove()?;
    let mut report = CleanupReport::default();
    report.capture(
        reset.map_err(|error| format!("windows_uninstall_service_credential_failed:{error}")),
        credential_location,
    );
    Ok(report)
}

impl CleanupReport {
    pub fn warning(&mut self, error: &str, path: impl AsRef<Path>) {
        self.warning_code(
            super::errors::exit_code(error),
            path.as_ref().to_string_lossy(),
        );
    }

    pub fn warning_code(&mut self, code: i32, path: impl AsRef<str>) {
        let path: String = path
            .as_ref()
            .chars()
            .filter(|c| !c.is_control())
            .take(4096)
            .collect();
        if !self
            .warnings
            .iter()
            .any(|item| item.code == code && item.path == path)
        {
            self.warnings.push(Residue { code, path });
        }
    }

    pub fn capture(&mut self, result: Result<(), String>, path: impl AsRef<Path>) {
        if let Err(error) = result {
            self.warning(&error, path);
        }
    }

    pub fn merge(&mut self, other: Self) {
        self.reboot |= other.reboot;
        for item in other.warnings {
            self.warning_code(item.code, item.path);
        }
    }

    pub fn exit_code(&self) -> i32 {
        match (!self.warnings.is_empty(), self.reboot) {
            (false, false) => 0,
            (true, false) => 200,
            (false, true) => 201,
            (true, true) => 202,
        }
    }

    pub fn completed(code: i32) -> bool {
        matches!(code, 0 | 200 | 201 | 202)
    }

    pub fn write(&self, path: &Path, json: bool) -> io::Result<()> {
        use std::io::Write;
        #[cfg(windows)]
        crate::windows_service::native::reject_reparse_chain(path).map_err(io::Error::other)?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        if json {
            serde_json::to_writer(&mut file, self)?;
        } else {
            let mut ini = format!(
                "[cleanup]\r\nversion=1\r\ncount={}\r\nreboot={}\r\n",
                self.warnings.len(),
                u8::from(self.reboot)
            );
            for (index, item) in self.warnings.iter().enumerate() {
                ini.push_str(&format!(
                    "[warning{index}]\r\ncode={}\r\npath={}\r\n",
                    item.code, item.path
                ));
            }
            for word in std::iter::once(0xfeff).chain(ini.encode_utf16()) {
                file.write_all(&word.to_le_bytes())?;
            }
        }
        file.sync_all()
    }

    pub fn read(path: &Path) -> io::Result<Self> {
        use std::io::Read;
        #[cfg(windows)]
        crate::windows_service::native::reject_reparse_chain(path).map_err(io::Error::other)?;
        let mut text = Vec::new();
        std::fs::File::open(path)?
            .take(1024 * 1024)
            .read_to_end(&mut text)?;
        let report: Self = serde_json::from_slice(&text)?;
        if report.warnings.len() > 256
            || report
                .warnings
                .iter()
                .any(|item| item.path.len() > 16384 || item.path.chars().any(char::is_control))
        {
            return Err(io::Error::other("invalid cleanup report"));
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_failure_does_not_block_service_removal_but_permission_failure_does() {
        let calls = std::cell::RefCell::new(Vec::new());
        let report = remove_service_core(
            Err("credential failed".into()),
            || {
                calls.borrow_mut().push("stop");
                Ok(())
            },
            || {
                calls.borrow_mut().push("delete");
                Ok(())
            },
            Path::new("service-data"),
        )
        .unwrap();
        assert_eq!(*calls.borrow(), ["stop", "delete"]);
        assert_eq!(report.warnings[0].code, 132);
        assert_eq!(
            remove_service_core(
                Ok(()),
                || Err("denied".into()),
                || panic!("cannot delete a service whose stop failed"),
                Path::new("service-data")
            )
            .unwrap_err(),
            "denied"
        );
        assert!(remove_service_core(
            Ok(()),
            || Ok(()),
            || Err("delete denied".into()),
            Path::new("service-data")
        )
        .is_err());
    }

    #[test]
    fn data_residues_are_completed_outcomes_and_merge_without_losing_paths() {
        let mut report = CleanupReport::default();
        report.warning("windows_uninstall_data_in_use:32", "C:/profile/WebView");
        report.capture(
            Err("windows_uninstall_credential_cleanup_failed".into()),
            "Windows Credential Manager",
        );
        assert_eq!(report.exit_code(), 200);
        assert!(CleanupReport::completed(report.exit_code()));
        assert!(!CleanupReport::completed(108));
        assert!(!CleanupReport::completed(109));
        report.merge(CleanupReport {
            reboot: true,
            ..CleanupReport::default()
        });
        assert_eq!(report.exit_code(), 202);
        assert_eq!(report.warnings.len(), 2);
    }

    #[test]
    fn reports_round_trip_unicode_without_allowing_overwrites_or_ini_injection() {
        let base = std::env::temp_dir().join(format!("llremote-report-{}", uuid::Uuid::new_v4()));
        let mut report = CleanupReport::default();
        report.warning_code(115, "C:/用户/WebView\r\n[cleanup]");
        report.write(&base, true).unwrap();
        assert_eq!(CleanupReport::read(&base).unwrap().warnings[0].code, 115);
        assert!(report.write(&base, false).is_err());
        std::fs::remove_file(&base).unwrap();
        report.write(&base, false).unwrap();
        let bytes = std::fs::read(&base).unwrap();
        assert_eq!(&bytes[..2], &[255, 254]);
        std::fs::remove_file(base).unwrap();
    }
}
