use super::*;

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn development_bundle_has_a_fixed_identity_and_uses_only_adhoc_signing() {
    let path =
        std::env::temp_dir().join(format!("vnc-sharing-bundle-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&path).unwrap();
    let directory = Directory(path);
    let source_code = directory.0.join("main.c");
    let source = directory.0.join("development-host");
    std::fs::write(&source_code, "int main(void) { return 0; }\n").unwrap();
    assert!(Command::new("xcrun")
        .arg("clang")
        .arg(&source_code)
        .arg("-o")
        .arg(&source)
        .status()
        .unwrap()
        .success());
    let target = prepare_development_bundle(&source, &directory.0).unwrap();
    assert!(is_app_executable(&target));
    let bundle = directory.0.join(BUNDLE_NAME);
    assert_eq!(bundle.file_name().unwrap(), "LLRemote Service.app");
    let contents = bundle.join("Contents");
    for (key, expected) in [
        ("CFBundleName", "LLRemote Service"),
        ("CFBundleDisplayName", "LLRemote Service"),
        ("CFBundleExecutable", "LLRemoteService"),
        ("CFBundleIconFile", "icon.icns"),
    ] {
        let value = Command::new("/usr/libexec/PlistBuddy")
            .args(["-c", &format!("Print :{key}")])
            .arg(contents.join("Info.plist"))
            .output()
            .unwrap();
        assert!(value.status.success());
        assert_eq!(String::from_utf8(value.stdout).unwrap().trim(), expected);
    }
    let icon = contents.join("Resources/icon.icns");
    assert_eq!(
        std::fs::read(&icon).unwrap(),
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/icons/icon.icns"))
    );
    assert!(Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict"])
        .arg(&bundle)
        .status()
        .unwrap()
        .success());
    let signature = Command::new("/usr/bin/codesign")
        .args(["-d", "--verbose=4"])
        .arg(&bundle)
        .output()
        .unwrap();
    let signature = String::from_utf8(signature.stderr).unwrap();
    assert!(signature.contains("Signature=adhoc"));
    assert!(signature.contains(&format!("Identifier={}", development_metadata().unwrap().1)));
    let before = std::fs::metadata(&target).unwrap().modified().unwrap();
    assert_eq!(
        prepare_development_bundle(&source, &directory.0).unwrap(),
        target
    );
    assert_eq!(
        std::fs::metadata(&target).unwrap().modified().unwrap(),
        before
    );
    std::fs::remove_file(&icon).unwrap();
    assert_eq!(
        prepare_development_bundle(&source, &directory.0).unwrap(),
        target
    );
    assert_eq!(std::fs::read(&icon).unwrap(), ICON);
    let old_hash = std::fs::read_to_string(directory.0.join("sharing-source.sha256")).unwrap();
    std::fs::write(&source_code, "int main(void) { return 1; }\n").unwrap();
    assert!(Command::new("xcrun")
        .arg("clang")
        .arg(&source_code)
        .arg("-o")
        .arg(&source)
        .status()
        .unwrap()
        .success());
    assert_eq!(
        prepare_development_bundle(&source, &directory.0).unwrap(),
        target
    );
    assert_ne!(
        std::fs::read_to_string(directory.0.join("sharing-source.sha256")).unwrap(),
        old_hash
    );
    assert!(Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict"])
        .arg(&bundle)
        .status()
        .unwrap()
        .success());
}

#[test]
fn packaged_main_executables_are_recognized_without_development_wrapping() {
    assert!(is_app_executable(Path::new(
        "/Applications/LL Remote.app/Contents/MacOS/LLRemote"
    )));
    assert!(!is_app_executable(Path::new("/project/target/debug/app")));
    assert!(!is_app_executable(Path::new(
        "/project/app/Contents/MacOS/app"
    )));
}

#[test]
fn packaged_app_never_launches_its_foreground_binary_as_the_background_host() {
    let path =
        std::env::temp_dir().join(format!("llremote-packaged-host-{}", uuid::Uuid::new_v4()));
    let directory = Directory(path);
    let contents = directory.0.join("LL Remote.app/Contents");
    let main = contents.join("MacOS/LLRemote");
    let service = contents.join("Helpers/LLRemote Service.app/Contents/MacOS/LLRemoteService");
    std::fs::create_dir_all(main.parent().unwrap()).unwrap();
    std::fs::write(&main, b"foreground").unwrap();
    assert_eq!(
        executable_for_source(&main).unwrap_err(),
        "background_share_bundle_unavailable"
    );
    std::fs::create_dir_all(service.parent().unwrap()).unwrap();
    std::fs::write(&service, b"background").unwrap();
    assert_eq!(executable_for_source(&main).unwrap(), service);
    assert_eq!(executable_for_source(&service).unwrap(), service);
}

#[test]
fn packaged_sharing_host_resolves_the_outer_app_for_screen_permission_repair() {
    let path = std::env::temp_dir().join(format!(
        "llremote-packaged-owner-test-{}",
        uuid::Uuid::new_v4()
    ));
    let directory = Directory(path);
    let contents = directory.0.join("LL Remote.app/Contents");
    let service = contents.join("Helpers/LLRemote Service.app/Contents/MacOS/LLRemoteService");
    std::fs::create_dir_all(service.parent().unwrap()).unwrap();
    std::fs::write(
        contents.join("Info.plist"),
        r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.example.LLRemote</string></dict></plist>"#,
    )
    .unwrap();
    assert_eq!(
        owning_application_identifier(&service).unwrap().as_deref(),
        Some("com.example.LLRemote")
    );
    assert_eq!(
        owning_application_identifier(
            &directory
                .0
                .join("LLRemote Service.app/Contents/MacOS/LLRemoteService")
        )
        .unwrap(),
        None
    );
}

#[test]
fn native_identity_matches_the_signed_host_without_requesting_system_permissions() {
    let path = std::env::temp_dir().join(format!(
        "vnc-sharing-identity-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir(&path).unwrap();
    let directory = Directory(path);
    let source = directory.0.join("identity.c");
    let binary = directory.0.join("identity-host");
    std::fs::write(
        &source,
        r#"
#include <stdbool.h>
#include <stddef.h>
#include <stdio.h>
extern bool vnc_sharing_host_identity(unsigned char *, size_t, unsigned char *, size_t);
int main(void) {
    unsigned char identifier[256] = {0}, hash[129] = {0};
    if (!vnc_sharing_host_identity(identifier, sizeof(identifier), hash, sizeof(hash))) return 1;
    printf("%s\n%s\n", identifier, hash); return 0;
}
"#,
    )
    .unwrap();
    assert!(Command::new("xcrun")
        .args(["clang", "-fobjc-arc", "-fblocks"])
        .arg(&source)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("native/macos/VncSharingHost.m"))
        .args([
            "-framework",
            "AppKit",
            "-framework",
            "ApplicationServices",
            "-framework",
            "Security",
            "-framework",
            "CoreServices",
            "-o"
        ])
        .arg(&binary)
        .status()
        .unwrap()
        .success());
    let target = prepare_development_bundle(&binary, &directory.0).unwrap();
    let result = Command::new(&target).output().unwrap();
    assert!(
        result.status.success(),
        "identity bridge rejected signed host"
    );
    let output = String::from_utf8(result.stdout).unwrap();
    let mut lines = output.lines();
    assert_eq!(
        lines.next(),
        Some(development_metadata().unwrap().1.as_str())
    );
    let hash = lines.next().unwrap();
    let signature = Command::new("/usr/bin/codesign")
        .args(["-d", "--verbose=4"])
        .arg(&target)
        .output()
        .unwrap();
    assert!(String::from_utf8(signature.stderr)
        .unwrap()
        .contains(&format!("CDHash={hash}\n")));
}
