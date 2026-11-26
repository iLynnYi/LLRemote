use super::FirewallOverview;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use windows_sys::Win32::NetworkManagement::NetManagement::{
    NetApiBufferFree, NetGetJoinInformation, NetSetupDomainName, NetSetupUnjoined,
    NetSetupUnknownStatus, NetSetupWorkgroupName,
};
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

fn system_directory() -> Result<PathBuf, String> {
    crate::windows_service::native::system_directory()
        .map_err(|_| "windows_firewall_unavailable".into())
}

fn powershell() -> Result<PathBuf, String> {
    Ok(system_directory()?.join(r"WindowsPowerShell\v1.0\powershell.exe"))
}

/// Query local membership without requiring administrator group membership or
/// contacting a domain controller. Unknown is never treated as workgroup.
fn domain_joined() -> Result<bool, String> {
    let mut name = std::ptr::null_mut();
    let mut status = NetSetupUnknownStatus;
    let result = unsafe { NetGetJoinInformation(std::ptr::null(), &mut name, &mut status) };
    if !name.is_null() {
        unsafe { NetApiBufferFree(name.cast()) };
    }
    if result != 0 {
        return Err("windows_firewall_network_types_unavailable".into());
    }
    if status == NetSetupDomainName {
        Ok(true)
    } else if status == NetSetupUnjoined || status == NetSetupWorkgroupName {
        Ok(false)
    } else {
        Err("windows_firewall_network_types_unavailable".into())
    }
}

fn encoded_script(domain_joined: bool) -> String {
    let source = format!(
        "$domainJoined = ${}\n{}",
        if domain_joined { "true" } else { "false" },
        include_str!("inspect.ps1")
    );
    let bytes: Vec<u8> = source.encode_utf16().flat_map(u16::to_le_bytes).collect();
    STANDARD.encode(bytes)
}

pub(super) fn inspect() -> Result<FirewallOverview, String> {
    let encoded = encoded_script(domain_joined()?);
    let mut child = Command::new(powershell()?)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand",
            &encoded,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "windows_firewall_unavailable".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "windows_firewall_inspect_failed".to_string())?;
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        stdout
            .take(1024 * 1024 + 1)
            .read_to_end(&mut output)
            .map(|_| output)
    });
    let deadline = Instant::now() + Duration::from_secs(25);
    let result = loop {
        match child.try_wait() {
            Ok(Some(exit)) if exit.success() => break Ok(()),
            Ok(Some(_)) => break Err("windows_firewall_inspect_failed"),
            Err(_) => break Err("windows_firewall_inspect_failed"),
            Ok(None) if Instant::now() >= deadline => break Err("windows_firewall_timeout"),
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let output = reader
        .join()
        .map_err(|_| "windows_firewall_inspect_failed".to_string())?
        .map_err(|_| "windows_firewall_inspect_failed".to_string())?;
    result.map_err(str::to_owned)?;
    if output.len() > 1024 * 1024 {
        return Err("windows_firewall_inspect_failed".into());
    }
    let snapshot = serde_json::from_slice::<FirewallOverview>(&output)
        .map_err(|_| "windows_firewall_inspect_failed".to_string())?;
    Ok(snapshot)
}

pub(super) fn open_settings() -> Result<(), String> {
    Command::new(system_directory()?.join("control.exe"))
        .args(["/name", "Microsoft.WindowsFirewall"])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|_| "windows_firewall_settings_failed".into())
}
