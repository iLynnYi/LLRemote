use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use LLRemoteLib::file_access::{
    grant_file_access_directory, inspect_file_access, list_file_access_directories,
    list_file_access_directory_statuses, prepare_builtin_file_access_directory,
    revoke_file_access_directory, set_builtin_file_access_directory, FileAccessGrantRequest,
    FileAccessPathRequest, FileAccessStore,
};

fn temp_access_root(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("llremote-file-access-{name}-{stamp}"))
}

#[test]
fn damaged_authorization_settings_are_not_overwritten_by_a_new_grant() {
    let root = temp_access_root("damaged");
    let allowed = root.join("captures");
    fs::create_dir_all(&allowed).unwrap();
    let path = root.join("file-access.json");
    for content in ["{", "null", "{}", "[{\"path\":7}]"] {
        fs::write(&path, content).unwrap();
        let store = FileAccessStore::open(path.clone()).unwrap();
        let error = grant_file_access_directory(
            &store,
            FileAccessGrantRequest {
                directory: allowed.to_string_lossy().into_owned(),
            },
        )
        .unwrap_err();
        assert_eq!(error, "file_access_settings_invalid");
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
        assert!(list_file_access_directories(&store).is_empty());
        let decision = inspect_file_access(
            &store,
            FileAccessPathRequest {
                path: allowed.to_string_lossy().into_owned(),
            },
        );
        assert_eq!(decision.status, "settings_unavailable");
        assert_eq!(decision.message, "file_access_settings_invalid");
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_failed_authorization_save_cannot_grant_access_only_in_memory() {
    let root = temp_access_root("failed-save");
    let allowed = root.join("captures");
    let settings = root.join("settings");
    fs::create_dir_all(&allowed).unwrap();
    fs::create_dir_all(&settings).unwrap();
    let store = FileAccessStore::open(settings.join("file-access.json")).unwrap();
    fs::remove_dir_all(&settings).unwrap();
    fs::write(&settings, b"blocks-directory-creation").unwrap();
    assert!(grant_file_access_directory(
        &store,
        FileAccessGrantRequest {
            directory: allowed.to_string_lossy().into_owned(),
        }
    )
    .is_err());
    assert_eq!(list_file_access_directories(&store).len(), 1);
    assert!(list_file_access_directories(&store)[0].is_builtin);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn grants_directories_persistently_and_authorizes_children() {
    let root = temp_access_root("grant");
    let allowed = root.join("captures");
    let nested = allowed.join("session-1");
    fs::create_dir_all(&nested).expect("create nested dir");
    fs::write(nested.join("shot.png"), b"image").expect("write artifact");

    let store = FileAccessStore::open(root.join("file-access.json")).expect("open store");
    let granted = grant_file_access_directory(
        &store,
        FileAccessGrantRequest {
            directory: allowed.to_string_lossy().to_string(),
        },
    )
    .expect("grant access");

    assert_eq!(
        granted.path,
        allowed.canonicalize().unwrap().to_string_lossy()
    );

    let restored = FileAccessStore::open(root.join("file-access.json")).expect("reopen store");
    assert_eq!(list_file_access_directories(&restored).len(), 2);

    let decision = inspect_file_access(
        &restored,
        FileAccessPathRequest {
            path: nested.join("shot.png").to_string_lossy().to_string(),
        },
    );

    assert_eq!(decision.status, "authorized");
    assert_eq!(
        decision.directory,
        allowed.canonicalize().unwrap().to_string_lossy()
    );
}

#[test]
fn reports_needed_authorization_for_paths_outside_granted_directories() {
    let root = temp_access_root("outside");
    let allowed = root.join("captures");
    let outside = root.join("exports");
    fs::create_dir_all(&allowed).expect("create allowed dir");
    fs::create_dir_all(&outside).expect("create outside dir");
    fs::write(outside.join("recording.webm"), b"video").expect("write artifact");

    let store = FileAccessStore::open(root.join("file-access.json")).expect("open store");
    grant_file_access_directory(
        &store,
        FileAccessGrantRequest {
            directory: allowed.to_string_lossy().to_string(),
        },
    )
    .expect("grant access");

    let decision = inspect_file_access(
        &store,
        FileAccessPathRequest {
            path: outside.join("recording.webm").to_string_lossy().to_string(),
        },
    );

    assert_eq!(decision.status, "needs_authorization");
    assert_eq!(
        decision.directory,
        outside.canonicalize().unwrap().to_string_lossy()
    );
}

#[test]
fn revokes_previously_granted_directory() {
    let root = temp_access_root("revoke");
    let allowed = root.join("captures");
    fs::create_dir_all(&allowed).expect("create allowed dir");

    let store = FileAccessStore::open(root.join("file-access.json")).expect("open store");
    grant_file_access_directory(
        &store,
        FileAccessGrantRequest {
            directory: allowed.to_string_lossy().to_string(),
        },
    )
    .expect("grant access");

    let removed = revoke_file_access_directory(
        &store,
        FileAccessGrantRequest {
            directory: allowed.to_string_lossy().to_string(),
        },
    )
    .expect("revoke access");

    assert!(removed);
    assert_eq!(list_file_access_directories(&store).len(), 1);
    assert!(list_file_access_directories(&store)[0].is_builtin);

    let decision = inspect_file_access(
        &store,
        FileAccessPathRequest {
            path: allowed.to_string_lossy().to_string(),
        },
    );
    assert_eq!(decision.status, "needs_authorization");
}

#[test]
fn keeps_missing_authorized_directories_visible_and_removable() {
    let root = temp_access_root("missing");
    let allowed = root.join("captures");
    fs::create_dir_all(&allowed).expect("create allowed dir");

    let store = FileAccessStore::open(root.join("file-access.json")).expect("open store");
    let granted = grant_file_access_directory(
        &store,
        FileAccessGrantRequest {
            directory: allowed.to_string_lossy().to_string(),
        },
    )
    .expect("grant access");

    fs::remove_dir(&allowed).expect("remove allowed dir");

    let statuses = list_file_access_directory_statuses(&store);
    assert_eq!(statuses.len(), 2);
    let missing = statuses
        .iter()
        .find(|entry| entry.path == granted.path)
        .unwrap();
    assert_eq!(missing.status, "missing");
    assert!(!missing.exists);

    let removed = revoke_file_access_directory(
        &store,
        FileAccessGrantRequest {
            directory: granted.path,
        },
    )
    .expect("revoke missing directory");

    assert!(removed);
    assert_eq!(list_file_access_directories(&store).len(), 1);
    assert!(list_file_access_directories(&store)[0].is_builtin);
}

#[test]
fn builtin_directory_is_created_first_and_cannot_be_revoked() {
    let root = temp_access_root("builtin");
    let store = FileAccessStore::open(root.join("file-access.json")).unwrap();
    let entries = list_file_access_directory_statuses(&store);
    assert_eq!(entries.len(), 1);
    assert!(entries[0].is_builtin);
    assert_eq!(entries[0].status, "ready");
    assert_eq!(
        PathBuf::from(&entries[0].path),
        root.join("Artifacts").canonicalize().unwrap()
    );
    assert_eq!(
        revoke_file_access_directory(
            &store,
            FileAccessGrantRequest {
                directory: entries[0].path.clone(),
            }
        )
        .unwrap_err(),
        "file_access_builtin_cannot_revoke"
    );
    let decision = inspect_file_access(
        &store,
        FileAccessPathRequest {
            path: root.to_string_lossy().into_owned(),
        },
    );
    assert_eq!(
        decision.status, "needs_authorization",
        "the app configuration parent is not granted"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_grants_remain_available_when_the_builtin_is_added() {
    let root = temp_access_root("legacy");
    let custom = root.join("custom");
    fs::create_dir_all(&custom).unwrap();
    let path = root.join("file-access.json");
    let legacy =
        serde_json::json!([{ "path": custom.canonicalize().unwrap(), "addedAt": "original" }])
            .to_string();
    fs::write(&path, &legacy).unwrap();
    let store = FileAccessStore::open(path.clone()).unwrap();
    let entries = list_file_access_directories(&store);
    assert_eq!(entries.len(), 2);
    assert!(entries[0].is_builtin);
    assert!(!entries[1].is_builtin);
    assert_eq!(entries[1].added_at, "original");
    assert_eq!(fs::read_to_string(path).unwrap(), legacy);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn changing_builtin_merges_an_existing_grant_and_preserves_old_files_and_authorization() {
    let root = temp_access_root("change-builtin");
    let custom = root.join("custom");
    fs::create_dir_all(&custom).unwrap();
    let path = root.join("file-access.json");
    let store = FileAccessStore::open(path.clone()).unwrap();
    let original = list_file_access_directories(&store)[0].clone();
    let old_file = PathBuf::from(&original.path).join("existing.png");
    fs::write(&old_file, b"keep-original-file").unwrap();
    let grant = grant_file_access_directory(
        &store,
        FileAccessGrantRequest {
            directory: custom.to_string_lossy().into_owned(),
        },
    )
    .unwrap();
    let updated = set_builtin_file_access_directory(
        &store,
        FileAccessGrantRequest {
            directory: grant.path.clone(),
        },
    )
    .unwrap();
    assert!(updated.is_builtin);
    assert_eq!(updated.path, grant.path);
    let restored = FileAccessStore::open(path).unwrap();
    let entries = list_file_access_directories(&restored);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries.iter().filter(|entry| entry.is_builtin).count(), 1);
    assert_eq!(entries[0].path, grant.path);
    assert!(!entries[1].is_builtin);
    assert_eq!(entries[1].path, original.path);
    assert_eq!(fs::read(&old_file).unwrap(), b"keep-original-file");
    assert_eq!(
        inspect_file_access(
            &restored,
            FileAccessPathRequest {
                path: old_file.to_string_lossy().into_owned()
            }
        )
        .status,
        "authorized"
    );
    assert!(revoke_file_access_directory(
        &restored,
        FileAccessGrantRequest {
            directory: original.path
        }
    )
    .unwrap());
    assert!(
        old_file.exists(),
        "revoking the old grant never deletes files"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn builtin_change_to_a_new_directory_is_persisted_and_invalid_changes_leave_it_intact() {
    let root = temp_access_root("new-builtin");
    let custom = root.join("new-location");
    fs::create_dir_all(&custom).unwrap();
    let settings = root.join("file-access.json");
    let store = FileAccessStore::open(settings.clone()).unwrap();
    let updated = set_builtin_file_access_directory(
        &store,
        FileAccessGrantRequest {
            directory: custom.to_string_lossy().into_owned(),
        },
    )
    .unwrap();
    let before = fs::read(&settings).unwrap();
    assert!(set_builtin_file_access_directory(
        &store,
        FileAccessGrantRequest {
            directory: root.join("missing").to_string_lossy().into_owned()
        }
    )
    .is_err());
    assert_eq!(fs::read(&settings).unwrap(), before);
    let restored = FileAccessStore::open(settings.clone()).unwrap();
    assert_eq!(list_file_access_directories(&restored)[0], updated);
    fs::remove_file(&settings).unwrap();
    fs::create_dir(&settings).unwrap();
    let original = root.join("Artifacts");
    assert!(set_builtin_file_access_directory(
        &store,
        FileAccessGrantRequest {
            directory: original.to_string_lossy().into_owned()
        }
    )
    .is_err());
    assert_eq!(
        list_file_access_directories(&store)[0],
        updated,
        "failed persistence cannot change in-memory ownership"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn deleted_builtin_is_recreated_without_recreating_missing_custom_directories() {
    let root = temp_access_root("recreate-builtin");
    let custom = root.join("custom");
    fs::create_dir_all(&custom).unwrap();
    let store = FileAccessStore::open(root.join("file-access.json")).unwrap();
    let builtin = list_file_access_directories(&store)[0].clone();
    grant_file_access_directory(
        &store,
        FileAccessGrantRequest {
            directory: custom.to_string_lossy().into_owned(),
        },
    )
    .unwrap();
    fs::remove_dir(&builtin.path).unwrap();
    fs::remove_dir(&custom).unwrap();
    prepare_builtin_file_access_directory(&store, &custom.to_string_lossy()).unwrap();
    assert!(!custom.exists());
    prepare_builtin_file_access_directory(&store, &builtin.path).unwrap();
    assert!(PathBuf::from(&builtin.path).is_dir());
    fs::remove_dir(&builtin.path).unwrap();
    assert_eq!(
        list_file_access_directory_statuses(&store)[0].status,
        "ready"
    );
    assert!(!custom.exists());
    fs::remove_dir_all(root).unwrap();
}
