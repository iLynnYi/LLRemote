use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Identify the compiled runtime, including uncommitted development changes.
    println!("cargo:rerun-if-changed=src");
    let build_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("build clock")
        .as_nanos();
    println!("cargo:rustc-env=VNC_SHARING_BUILD_ID={build_id}");
    build_macos_screen_capture();
    if env::var("TARGET").as_deref() == Ok("x86_64-apple-darwin") {
        // Tauri signs the main executable before its sibling service. The Intel
        // linker needs an explicit initial signature on that service.
        println!("cargo:rustc-link-arg-bin=LLRemoteService=-Wl,-adhoc_codesign");
    }
    println!("cargo:rerun-if-changed=windows/app.manifest");
    let windows =
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("windows/app.manifest"));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to build Tauri application resources")
}

fn build_macos_screen_capture() {
    println!("cargo:rerun-if-changed=native/macos/VncScreenCapture.m");
    println!("cargo:rerun-if-changed=native/macos/VncSharingHost.m");
    println!("cargo:rerun-if-changed=native/macos/RemoteMessage.m");
    println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
    if env::var("CARGO_CFG_TARGET_OS").ok().as_deref() != Some("macos") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        _ => panic!("unsupported macOS capture architecture"),
    };
    let minimum = env::var("MACOSX_DEPLOYMENT_TARGET").unwrap_or_else(|_| "15.0".into());
    println!("cargo:rustc-env=MACOSX_DEPLOYMENT_TARGET={minimum}");
    println!("cargo:rustc-env=LLREMOTE_MACOS_MINIMUM={minimum}");
    let object = out.join("vnc-screen-capture.o");
    let host_object = out.join("vnc-sharing-host.o");
    let message_object = out.join("remote-message.o");
    let archive = out.join("libvnc_screen_capture.a");
    let compiled = Command::new("xcrun")
        .args([
            "clang",
            "-fobjc-arc",
            "-fblocks",
            "-Wall",
            "-Werror",
            "-arch",
            arch,
        ])
        .arg(format!("-mmacosx-version-min={minimum}"))
        .args(["-c", "native/macos/VncScreenCapture.m", "-o"])
        .arg(&object)
        .status()
        .expect("macOS clang");
    assert!(
        compiled.success(),
        "macOS screen capture bridge compilation failed"
    );
    let host_compiled = Command::new("xcrun")
        .args([
            "clang",
            "-fobjc-arc",
            "-fblocks",
            "-Wall",
            "-Werror",
            "-arch",
            arch,
        ])
        .arg(format!("-mmacosx-version-min={minimum}"))
        .args(["-c", "native/macos/VncSharingHost.m", "-o"])
        .arg(&host_object)
        .status()
        .expect("macOS sharing host clang");
    assert!(
        host_compiled.success(),
        "macOS sharing host bridge compilation failed"
    );
    let message_compiled = Command::new("xcrun")
        .args([
            "clang",
            "-fobjc-arc",
            "-fblocks",
            "-Wall",
            "-Werror",
            "-arch",
            arch,
        ])
        .arg(format!("-mmacosx-version-min={minimum}"))
        .args(["-c", "native/macos/RemoteMessage.m", "-o"])
        .arg(&message_object)
        .status()
        .expect("macOS remote message clang");
    assert!(
        message_compiled.success(),
        "macOS remote message bridge compilation failed"
    );
    let archived = Command::new("xcrun")
        .args(["ar", "crs"])
        .arg(&archive)
        .arg(&object)
        .arg(&host_object)
        .arg(&message_object)
        .status()
        .expect("macOS ar");
    assert!(
        archived.success(),
        "macOS screen capture bridge archive failed"
    );
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=vnc_screen_capture");
    for framework in [
        "Foundation",
        "CoreMedia",
        "CoreVideo",
        "CoreGraphics",
        "AppKit",
        "Security",
        "CoreServices",
        "ScreenCaptureKit",
    ] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}
