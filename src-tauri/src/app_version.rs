//! Protect user stores when a DMG is replaced with an older app (or a copied
//! executable bypasses the Windows installer). Maintenance never opens stores.
use semver::Version;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

pub(crate) fn check_and_record(directory: &Path, current: &str) -> Result<(), String> {
    let current = Version::parse(current).map_err(|_| "应用版本无效，已停止启动。".to_string())?;
    let path = directory.join(".app-version");
    #[cfg(windows)]
    crate::windows_storage::check_path(&path)
        .map_err(|_| "无法安全访问应用版本记录，已停止启动。".to_string())?;
    fs::create_dir_all(directory).map_err(|_| "无法访问应用数据目录，已停止启动。".to_string())?;
    let (mut file, created) = match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(file) => (file, true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if fs::symlink_metadata(&path)
                .map_err(|_| "无法读取版本记录。")?
                .file_type()
                .is_symlink()
            {
                return Err("版本记录不能是符号链接，已停止启动。".into());
            }
            (
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path)
                    .map_err(|_| "无法访问版本记录，已停止启动。")?,
                false,
            )
        }
        Err(_) => return Err("无法创建版本记录，已停止启动。".into()),
    };
    file.lock().map_err(|_| "无法锁定版本记录，已停止启动。")?;
    let mut saved = String::new();
    file.read_to_string(&mut saved)
        .map_err(|_| "无法读取版本记录，已停止启动。")?;
    if !created {
        let previous = Version::parse(saved.trim())
            .map_err(|_| "版本记录损坏，无法安全判断是否降级。请检查备份后重试。")?;
        match current.cmp_precedence(&previous) {
            std::cmp::Ordering::Less => return Err(format!(
                "此电脑的应用数据已由 LL Remote {previous} 使用，当前版本为 {current}。为保护数据，已阻止降级。请安装 {previous} 或更新版本；如需回退，请先备份并使用与旧版兼容的数据。"
            )),
            std::cmp::Ordering::Equal => return Ok(()),
            std::cmp::Ordering::Greater => {}
        }
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| "无法更新版本记录。")?;
    file.set_len(0).map_err(|_| "无法更新版本记录。")?;
    file.write_all(current.to_string().as_bytes())
        .map_err(|_| "无法更新版本记录。")?;
    file.sync_all()
        .map_err(|_| "无法保存版本记录。".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrades_and_repairs_preserve_data_and_block_older_apps() {
        let directory =
            std::env::temp_dir().join(format!("llremote-version-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("hosts.sqlite3"), "user data").unwrap();
        check_and_record(&directory, "0.12.0").unwrap();
        check_and_record(&directory, "0.12.0+rebuild").unwrap();
        check_and_record(&directory, "0.13.0-beta.1").unwrap();
        assert!(check_and_record(&directory, "0.12.0")
            .unwrap_err()
            .contains("已阻止降级"));
        check_and_record(&directory, "0.13.0").unwrap();
        assert!(check_and_record(&directory, "0.13.0-beta.2").is_err());
        assert_eq!(
            fs::read_to_string(directory.join(".app-version")).unwrap(),
            "0.13.0"
        );
        assert_eq!(
            fs::read_to_string(directory.join("hosts.sqlite3")).unwrap(),
            "user data"
        );
        fs::write(directory.join(".app-version"), "broken").unwrap();
        assert!(check_and_record(&directory, "0.14.0")
            .unwrap_err()
            .contains("记录损坏"));
        fs::remove_dir_all(directory).unwrap();
    }
}
