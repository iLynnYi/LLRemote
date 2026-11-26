//! Per-user startup registration; neither platform creates a system daemon here.
use super::{
    paths,
    types::{BackgroundShareLaunchMode, StartupRegistration},
};
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub(super) struct LaunchedHost {
    #[cfg(target_os = "windows")]
    process: super::startup_windows::Process,
}

impl LaunchedHost {
    pub(super) fn check_running(&self) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        return self.process.check_running();
        #[cfg(not(target_os = "windows"))]
        Ok(())
    }
}

#[cfg(target_os = "windows")]
pub(super) fn executable() -> Result<PathBuf, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let source =
        std::env::current_exe().map_err(|_| "background_share_binary_unavailable".to_string())?;
    let directory = paths::data_directory()?.join("bin");
    paths::ensure_private_directory(&directory)?;
    if source.parent() == Some(directory.as_path()) {
        return Ok(source);
    }
    let mut file = std::fs::File::open(&source)
        .map_err(|_| "background_share_binary_unavailable".to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| "background_share_binary_unavailable".to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let hash = hex::encode(hasher.finalize());
    let target = directory.join(format!("LLRemoteService-{}.exe", &hash[..24]));
    if !target.exists() {
        std::fs::copy(source, &target)
            .map_err(|_| "background_share_binary_unavailable".to_string())?;
    }
    Ok(target)
}

#[cfg(target_os = "macos")]
pub(super) fn executable() -> Result<PathBuf, String> {
    super::bundle_macos::executable()
}

#[cfg(target_os = "macos")]
const LABEL: &str = "com.iLynnYi.LLRemote.sharing";

pub(super) fn registration() -> StartupRegistration {
    #[cfg(target_os = "macos")]
    {
        let Some(path) = std::env::var_os("HOME").map(PathBuf::from).map(|home| {
            home.join("Library/LaunchAgents")
                .join(format!("{LABEL}.plist"))
        }) else {
            return StartupRegistration::Unknown;
        };
        if !path.is_file() {
            return StartupRegistration::Missing;
        }
        macos_registration(&path)
    }
    #[cfg(target_os = "windows")]
    {
        windows_registration()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    StartupRegistration::Unknown
}

#[cfg(target_os = "macos")]
fn macos_registration(path: &std::path::Path) -> StartupRegistration {
    use std::ffi::{c_void, CString};
    type Id = *mut c_void;
    type Sel = *mut c_void;
    #[link(name = "objc")]
    unsafe extern "C" {
        fn objc_getClass(name: *const i8) -> Id;
        fn sel_registerName(name: *const i8) -> Sel;
        fn objc_msgSend();
    }
    unsafe extern "C" {
        fn dlopen(path: *const i8, mode: i32) -> *mut c_void;
    }
    #[link(name = "Foundation", kind = "framework")]
    unsafe extern "C" {}
    // statusForLegacyURL: is available on macOS 13+. An unrecognized legacy
    // job is unknown, never assumed enabled from the plist alone.
    unsafe {
        static LOADED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if !*LOADED.get_or_init(|| {
            !dlopen(
                c"/System/Library/Frameworks/ServiceManagement.framework/ServiceManagement"
                    .as_ptr(),
                1,
            )
            .is_null()
        }) {
            return StartupRegistration::Unknown;
        }
        let class = |name: &'static [u8]| objc_getClass(name.as_ptr().cast());
        let selector = |name: &'static [u8]| sel_registerName(name.as_ptr().cast());
        let service = class(b"SMAppService\0");
        if service.is_null() {
            return StartupRegistration::Unknown;
        }
        let pool_class = class(b"NSAutoreleasePool\0");
        let string_class = class(b"NSString\0");
        let url_class = class(b"NSURL\0");
        if pool_class.is_null() || string_class.is_null() || url_class.is_null() {
            return StartupRegistration::Unknown;
        }
        let send_id: extern "C" fn(Id, Sel) -> Id = std::mem::transmute(objc_msgSend as *const ());
        let send_arg: extern "C" fn(Id, Sel, Id) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_cstr: extern "C" fn(Id, Sel, *const i8) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_status: extern "C" fn(Id, Sel, Id) -> isize =
            std::mem::transmute(objc_msgSend as *const ());
        let send_bool: extern "C" fn(Id, Sel, Sel) -> i8 =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void: extern "C" fn(Id, Sel) = std::mem::transmute(objc_msgSend as *const ());
        let status_selector = selector(b"statusForLegacyURL:\0");
        if send_bool(service, selector(b"respondsToSelector:\0"), status_selector) == 0 {
            return StartupRegistration::Unknown;
        }
        let pool = send_id(pool_class, selector(b"new\0"));
        let name = match CString::new(path.to_string_lossy().as_bytes()) {
            Ok(name) => name,
            Err(_) => {
                send_void(pool, selector(b"drain\0"));
                return StartupRegistration::Unknown;
            }
        };
        let string = send_cstr(
            string_class,
            selector(b"stringWithUTF8String:\0"),
            name.as_ptr(),
        );
        let url = send_arg(url_class, selector(b"fileURLWithPath:\0"), string);
        let state = if url.is_null() {
            -1
        } else {
            send_status(service, status_selector, url)
        };
        send_void(pool, selector(b"drain\0"));
        match state {
            1 => StartupRegistration::Enabled,
            2 => StartupRegistration::Disabled,
            _ => StartupRegistration::Unknown,
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_registration() -> StartupRegistration {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_BINARY,
    };
    let Some(directory) = std::env::var_os("APPDATA").map(PathBuf::from) else {
        return StartupRegistration::Unknown;
    };
    if !directory
        .join("Microsoft/Windows/Start Menu/Programs/Startup/LLRemote Sharing.lnk")
        .is_file()
    {
        return StartupRegistration::Missing;
    }
    let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let key = wide(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\StartupFolder",
    );
    let name = wide("LLRemote Sharing.lnk");
    let mut data = [0_u8; 16];
    let mut size = data.len() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_BINARY,
            std::ptr::null_mut(),
            data.as_mut_ptr().cast(),
            &mut size,
        )
    };
    match result {
        0 if size >= 1 => match data[0] {
            2 => StartupRegistration::Enabled,
            3 => StartupRegistration::Disabled,
            _ => StartupRegistration::Unknown,
        },
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => StartupRegistration::Enabled,
        _ => StartupRegistration::Unknown,
    }
}

#[cfg(target_os = "macos")]
pub(super) fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "macos")]
fn plist(mode: BackgroundShareLaunchMode) -> Result<String, String> {
    let executable = executable()?;
    let association = xml_escape(&super::bundle_macos::associated_identifier(&executable)?);
    let program = xml_escape(&executable.to_string_lossy());
    let argument = mode.argument();
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>Label</key><string>{LABEL}</string>
<key>AssociatedBundleIdentifiers</key><array><string>{association}</string></array>
<key>ProgramArguments</key><array><string>{program}</string><string>{argument}</string></array>
<key>RunAtLoad</key><{run}/><key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
<key>ThrottleInterval</key><integer>5</integer><key>LimitLoadToSessionType</key><string>Aqua</string>
</dict></plist>"#,
        run = if mode == BackgroundShareLaunchMode::Login {
            "true"
        } else {
            "false"
        }
    ))
}

pub(super) fn configure(enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let directory = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "background_share_startup_unavailable".to_string())?
            .join("Library/LaunchAgents");
        let path = directory.join(format!("{LABEL}.plist"));
        if enabled {
            std::fs::create_dir_all(&directory)
                .map_err(|_| "background_share_startup_unavailable".to_string())?;
            std::fs::write(path, plist(BackgroundShareLaunchMode::Login)?)
                .map_err(|_| "background_share_startup_unavailable".to_string())?;
        } else if path.exists() {
            std::fs::remove_file(path)
                .map_err(|_| "background_share_startup_unavailable".to_string())?;
        }
    }
    #[cfg(target_os = "windows")]
    {
        use base64::{engine::general_purpose, Engine};
        use std::os::windows::process::CommandExt;
        let startup_directory = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| "background_share_startup_unavailable".to_string())?
            .join("Microsoft/Windows/Start Menu/Programs/Startup");
        let startup = startup_directory.join("LLRemote Sharing.lnk");
        if !enabled {
            match std::fs::remove_file(startup) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(_) => return Err("background_share_startup_unavailable".into()),
            }
        }
        let program = executable()?;
        let script = format!("$ErrorActionPreference='Stop';$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.Arguments='--background-share';$s.WindowStyle=7;$s.Save()", startup.to_string_lossy().replace('\'', "''"), program.to_string_lossy().replace('\'', "''"));
        let bytes = script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let status = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
                &general_purpose::STANDARD.encode(bytes),
            ])
            .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| "background_share_startup_unavailable".to_string())?;
        if !status.success() {
            return Err("background_share_startup_unavailable".into());
        }
    }
    Ok(())
}

pub(super) fn launch() -> Result<LaunchedHost, String> {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn geteuid() -> u32;
        }
        let domain = format!("gui/{}", unsafe { geteuid() });
        let directory = paths::data_directory()?;
        paths::ensure_private_directory(&directory)?;
        let path = directory.join("sharing-job.plist");
        std::fs::write(&path, plist(BackgroundShareLaunchMode::Idle)?)
            .map_err(|_| "background_share_startup_unavailable".to_string())?;
        let loaded = Command::new("launchctl")
            .args(["print", &format!("{domain}/{LABEL}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| "background_share_startup_unavailable".to_string())?
            .success();
        // Called only when the control endpoint is absent. Reload the job so a
        // moved App or a rebuilt development binary uses its current path.
        if loaded {
            let status = Command::new("launchctl")
                .args(["bootout", &format!("{domain}/{LABEL}")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|_| "background_share_startup_unavailable".to_string())?;
            if !status.success() {
                return Err("background_share_startup_unavailable".into());
            }
        }
        let status = Command::new("launchctl")
            .args(["bootstrap", &domain])
            .arg(&path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| "background_share_startup_unavailable".to_string())?;
        if !status.success() {
            return Err("background_share_startup_unavailable".into());
        }
        let status = Command::new("launchctl")
            .args(["kickstart", &format!("{domain}/{LABEL}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| "background_share_startup_unavailable".to_string())?;
        if !status.success() {
            return Err("background_share_startup_unavailable".into());
        }
    }
    #[cfg(target_os = "windows")]
    let process =
        super::startup_windows::launch(&executable()?, BackgroundShareLaunchMode::Idle.argument())?;
    Ok(LaunchedHost {
        #[cfg(target_os = "windows")]
        process,
    })
}

// Explicit stops remove the loaded job as well as the process. The saved login
// registration is retained; it may start sharing on the next login only.
pub(super) fn unload() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn geteuid() -> u32;
        }
        let service = format!("gui/{}/{LABEL}", unsafe { geteuid() });
        let loaded = Command::new("launchctl")
            .args(["print", &service])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| "background_share_startup_unavailable")?
            .success();
        if loaded
            && !Command::new("launchctl")
                .args(["bootout", &service])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|_| "background_share_startup_unavailable")?
                .success()
        {
            return Err("background_share_startup_unavailable".into());
        }
    }
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
mod registration_tests {
    use super::*;

    #[test]
    fn an_unregistered_legacy_plist_is_not_reported_as_enabled() {
        let path =
            std::env::temp_dir().join(format!("unregistered-share-{}.plist", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"<plist/>").unwrap();
        assert_ne!(macos_registration(&path), StartupRegistration::Enabled);
        std::fs::remove_file(path).unwrap();
    }
}
