impl FileAccessStore {
    pub fn open(file_path: PathBuf) -> Result<Self, String> {
        let builtin = file_path.parent().ok_or("file_access_settings_path_invalid")?.join("Artifacts");
        Self::open_with_builtin(file_path, builtin)
    }

    pub fn open_with_builtin(file_path: PathBuf, builtin: PathBuf) -> Result<Self, String> {
        let (directories, settings_error) = match fs::read_to_string(&file_path) {
            Ok(content) => match serde_json::from_str::<Vec<FileAccessDirectory>>(&content) {
                Ok(entries) if entries.iter().all(|entry| Path::new(&entry.path).is_absolute())
                    && entries.iter().filter(|entry| entry.is_builtin).count() <= 1 => (entries, None),
                _ => (Vec::new(), Some("file_access_settings_invalid")),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Vec::new(), None),
            Err(_) => (Vec::new(), Some("file_access_settings_unavailable")),
        };

        let mut directories = directories;
        if settings_error.is_none() {
            if !directories.iter().any(|entry| entry.is_builtin) {
                let parent = file_path.parent().ok_or("file_access_settings_path_invalid")?;
                fs::create_dir_all(parent).map_err(|error| format!("file_access_directory_create_failed: {error}"))?;
                let directory = builtin.clone();
                // Keep the application usable when the location is unavailable; status reports it.
                let _ = fs::create_dir_all(&directory);
                let path = directory.canonicalize().unwrap_or(directory).to_string_lossy().to_string();
                if let Some(existing) = directories.iter_mut().find(|entry| entry.path == path) {
                    existing.is_builtin = true;
                } else {
                    directories.push(FileAccessDirectory { path, added_at: now_stamp(), is_builtin: true });
                }
            }
            sort_directories(&mut directories);
            if let Some(builtin) = directories.iter().find(|entry| entry.is_builtin) {
                let _ = fs::create_dir_all(&builtin.path);
            }
        }

        Ok(Self {
            file_path,
            directories: Mutex::new(directories),
            settings_error,
        })
    }

    /// Keeps malformed grants fail-closed while leaving the app and recovery UI available.
    pub fn ensure_available(&self) -> Result<(), String> {
        self.settings_error.map_or(Ok(()), |code| Err(code.into()))
    }

    fn save(&self, directories: &[FileAccessDirectory]) -> Result<(), String> {
        self.ensure_available()?;
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!("failed to create file access settings directory: {error}")
            })?;
        }

        let content = serde_json::to_string_pretty(directories)
            .map_err(|error| format!("failed to encode file access settings: {error}"))?;
        fs::write(&self.file_path, content)
            .map_err(|error| format!("failed to save file access settings: {error}"))
    }
}
