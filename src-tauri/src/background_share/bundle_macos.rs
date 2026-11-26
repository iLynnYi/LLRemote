//! Resolve the separately identified sharing app in development and packages.
//! Ad-hoc signing needs no certificate and does not promise grant persistence
//! across backend rebuilds. Packaged apps retain their existing signature.
use super::paths;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SERVICE_NAME: &str = "LLRemote Service";
const BUNDLE_NAME: &str = "LLRemote Service.app";
const EXECUTABLE_NAME: &str = "LLRemoteService";
const ICON_NAME: &str = "icon.icns";
const MINIMUM_SYSTEM_VERSION: &str = env!("LLREMOTE_MACOS_MINIMUM");
const ICON: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/icons/icon.icns"));

pub(super) fn is_app_executable(path: &Path) -> bool {
    path.parent().is_some_and(|macos| {
        macos.file_name().is_some_and(|name| name == "MacOS")
            && macos.parent().is_some_and(|contents| {
                contents.file_name().is_some_and(|name| name == "Contents")
                    && contents.parent().is_some_and(|bundle| {
                        bundle
                            .extension()
                            .is_some_and(|extension| extension == "app")
                    })
            })
    })
}

pub(super) fn development_executable() -> Result<PathBuf, String> {
    Ok(paths::data_directory()?
        .join(BUNDLE_NAME)
        .join("Contents/MacOS")
        .join(EXECUTABLE_NAME))
}

pub(super) fn executable() -> Result<PathBuf, String> {
    let source =
        std::env::current_exe().map_err(|_| "background_share_binary_unavailable".to_string())?;
    executable_for_source(&source)
}

fn executable_for_source(source: &Path) -> Result<PathBuf, String> {
    if let Some(service) = packaged_executable(source) {
        return service
            .is_file()
            .then_some(service)
            .ok_or_else(|| "background_share_bundle_unavailable".into());
    }
    prepare_development_bundle(source, &paths::data_directory()?)
}

pub(super) fn packaged_executable(source: &Path) -> Option<PathBuf> {
    if !is_app_executable(source) {
        return None;
    }
    let contents = source.parent()?.parent()?;
    if contents.parent()?.file_name()? == BUNDLE_NAME {
        Some(source.to_path_buf())
    } else {
        Some(
            contents
                .join("Helpers")
                .join(BUNDLE_NAME)
                .join("Contents/MacOS")
                .join(EXECUTABLE_NAME),
        )
    }
}

fn development_metadata() -> Result<(String, String), String> {
    let config: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tauri.conf.json"
    )))
    .map_err(|_| "background_share_bundle_unavailable".to_string())?;
    let identifier = config["identifier"]
        .as_str()
        .ok_or("background_share_bundle_unavailable")?;
    Ok((SERVICE_NAME.into(), format!("{identifier}.sharing")))
}

pub(super) fn associated_identifier(executable: &Path) -> Result<String, String> {
    if executable == development_executable()? {
        return Ok(development_metadata()?.1);
    }
    let contents = executable
        .parent()
        .and_then(Path::parent)
        .ok_or("background_share_bundle_unavailable")?;
    plist_identifier(contents)
}

/// A packaged helper can be attributed to its containing app for ScreenCapture
/// even though Accessibility is shown under the helper's own bundle name.
pub(super) fn owning_application_identifier(executable: &Path) -> Result<Option<String>, String> {
    if !is_app_executable(executable) {
        return Err("background_share_bundle_unavailable".into());
    }
    let service_bundle = executable
        .ancestors()
        .nth(3)
        .ok_or("background_share_bundle_unavailable")?;
    if service_bundle.file_name() != Some(std::ffi::OsStr::new(BUNDLE_NAME)) {
        return Ok(None);
    }
    let helpers = service_bundle
        .parent()
        .ok_or("background_share_bundle_unavailable")?;
    if helpers.file_name() != Some(std::ffi::OsStr::new("Helpers")) {
        return Ok(None);
    }
    let contents = helpers
        .parent()
        .ok_or("background_share_bundle_unavailable")?;
    if contents.file_name() != Some(std::ffi::OsStr::new("Contents"))
        || contents.parent().and_then(Path::extension) != Some(std::ffi::OsStr::new("app"))
    {
        return Err("background_share_bundle_unavailable".into());
    }
    plist_identifier(contents).map(Some)
}

fn plist_identifier(contents: &Path) -> Result<String, String> {
    let output = Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", "Print :CFBundleIdentifier"])
        .arg(contents.join("Info.plist"))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| "background_share_bundle_unavailable".to_string())?;
    if !output.status.success() {
        return Err("background_share_bundle_unavailable".into());
    }
    let identifier = String::from_utf8(output.stdout)
        .map_err(|_| "background_share_bundle_unavailable".to_string())?;
    if identifier.trim().is_empty() {
        return Err("background_share_bundle_unavailable".into());
    }
    Ok(identifier.trim().into())
}

fn prepare_development_bundle(source: &Path, directory: &Path) -> Result<PathBuf, String> {
    let failed = |_| "background_share_bundle_unavailable".to_string();
    paths::ensure_private_directory(directory)?;
    let (name, identifier) = development_metadata()?;
    let mut file = std::fs::File::open(source).map_err(failed)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(failed)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    // Branding changes must refresh a cached bundle even with the same source.
    hasher.update(name.as_bytes());
    hasher.update(identifier.as_bytes());
    hasher.update(MINIMUM_SYSTEM_VERSION.as_bytes());
    hasher.update(ICON);
    let hash = hex::encode(hasher.finalize());
    let bundle = directory.join(BUNDLE_NAME);
    let contents = bundle.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");
    let target = macos.join(EXECUTABLE_NAME);
    let marker = directory.join("sharing-source.sha256");
    if std::fs::read_to_string(&marker).ok().as_deref() == Some(hash.as_str())
        && target.is_file()
        && contents.join("Info.plist").is_file()
        && resources.join(ICON_NAME).is_file()
        && Command::new("/usr/bin/codesign")
            .args(["--verify", "--strict"])
            .arg(&bundle)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    {
        return Ok(target);
    }
    for path in [&bundle, &contents, &macos, &resources] {
        paths::ensure_private_directory(path)?;
    }
    let name = super::startup::xml_escape(&name);
    let escaped_identifier = super::startup::xml_escape(&identifier);
    std::fs::write(contents.join("Info.plist"), format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>{escaped_identifier}</string>
<key>CFBundleName</key><string>{name}</string>
<key>CFBundleDisplayName</key><string>{name}</string>
<key>CFBundleExecutable</key><string>{EXECUTABLE_NAME}</string>
<key>CFBundleIconFile</key><string>{ICON_NAME}</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSMinimumSystemVersion</key><string>{MINIMUM_SYSTEM_VERSION}</string>
<key>CFBundleVersion</key><string>{version}</string>
<key>CFBundleShortVersionString</key><string>{version}</string>
<key>NSScreenCaptureUsageDescription</key><string>用于将本机屏幕共享给已通过 VNC 验证的设备。</string>
<key>LSUIElement</key><true/>
</dict></plist>"#, version = env!("CARGO_PKG_VERSION"))).map_err(failed)?;
    std::fs::write(resources.join(ICON_NAME), ICON).map_err(failed)?;
    let staging = macos.join("sharing-host.next");
    std::fs::copy(source, &staging).map_err(failed)?;
    paths::replace_file(&staging, &target)?;
    let status = Command::new("/usr/bin/codesign")
        .args(["--force", "--sign", "-", "--identifier", &identifier])
        .arg(&bundle)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(failed)?;
    if !status.success() {
        return Err("background_share_bundle_unavailable".into());
    }
    std::fs::write(marker, hash).map_err(failed)?;
    Ok(target)
}

#[cfg(test)]
#[path = "../../tests/support/background_share_bundle_tests.rs"]
mod tests;
