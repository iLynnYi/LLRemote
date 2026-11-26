use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use LLRemoteLib::credential_vault::{CredentialSecret, CredentialVault};
use LLRemoteLib::host_store::{CreateHostRequest, HostRepository, UpdateHostRequest};

#[derive(Clone, Default)]
struct RecordingVault {
    stored: Arc<Mutex<Vec<CredentialSecret>>>,
    deleted: Arc<Mutex<Vec<String>>>,
}

impl CredentialVault for RecordingVault {
    fn store_host_password(&self, secret: CredentialSecret) -> Result<String, String> {
        let credential_ref = format!("test-vault://host/{}", secret.host_id);
        self.stored.lock().expect("stored lock").push(secret);
        Ok(credential_ref)
    }

    fn delete_host_password(&self, credential_ref: &str) -> Result<(), String> {
        self.deleted
            .lock()
            .expect("deleted lock")
            .push(credential_ref.to_string());
        Ok(())
    }

    fn get_host_password(&self, credential_ref: &str) -> Result<String, String> {
        let host_id = credential_ref
            .strip_prefix("test-vault://host/")
            .ok_or_else(|| "unsupported test credential ref".to_string())?;
        self.stored
            .lock()
            .expect("stored lock")
            .iter()
            .rev()
            .find(|secret| secret.host_id == host_id)
            .map(|secret| secret.password.clone())
            .ok_or_else(|| {
                format!("no saved password found for credential reference {credential_ref}")
            })
    }
}

impl RecordingVault {
    fn stored(&self) -> Vec<CredentialSecret> {
        self.stored.lock().expect("stored lock").clone()
    }

    fn deleted(&self) -> Vec<String> {
        self.deleted.lock().expect("deleted lock").clone()
    }
}

fn temp_db_path(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("llremote-{name}-{stamp}.sqlite3"))
}

fn host_request(save_password: bool) -> CreateHostRequest {
    CreateHostRequest {
        name: "lab-mac".to_string(),
        address: "127.0.0.1".to_string(),
        port: 5900,
        protocol: "vnc".to_string(),
        group: "lab".to_string(),

        save_password,
        password: Some("super-secret-password".to_string()),
    }
}

fn host_request_with_group(group: &str) -> CreateHostRequest {
    CreateHostRequest {
        name: format!("{group}-host"),
        address: "127.0.0.1".to_string(),
        port: 5900,
        protocol: "vnc".to_string(),
        group: group.to_string(),
        save_password: false,
        password: None,
    }
}

#[test]
fn host_repository_initializes_and_reopens_the_first_schema() {
    let db_path = temp_db_path("first-schema");
    let repository = HostRepository::open(&db_path).unwrap();
    assert_eq!(repository.list_groups().unwrap().len(), 1);
    repository.create_host(host_request(false)).unwrap();
    drop(repository);

    assert_eq!(
        HostRepository::open(&db_path)
            .unwrap()
            .list_hosts()
            .unwrap()
            .len(),
        1
    );
    let connection = Connection::open(&db_path).unwrap();
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
            .unwrap(),
        1
    );
}

#[test]
fn host_repository_rejects_a_newer_schema_without_downgrading_it() {
    let db_path = temp_db_path("newer-schema");
    let connection = Connection::open(&db_path).unwrap();
    connection.pragma_update(None, "user_version", 2).unwrap();
    assert!(HostRepository::open(&db_path).is_err());
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
            .unwrap(),
        2
    );
}

#[test]
fn host_repository_persists_hosts_across_reopen() {
    let db_path = temp_db_path("persist");
    let repository = HostRepository::open(&db_path).expect("open db");

    let created = repository
        .create_host(host_request(false))
        .expect("create host");
    assert_eq!(created.name, "lab-mac");
    assert_eq!(created.credential_ref, None);

    drop(repository);

    let reopened = HostRepository::open(&db_path).expect("reopen db");
    let hosts = reopened.list_hosts().expect("list hosts");

    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].id, created.id);
    assert_eq!(hosts[0].address, "127.0.0.1");
}

#[test]
fn host_repository_does_not_write_password_when_save_password_is_false() {
    let db_path = temp_db_path("no-password");
    let repository = HostRepository::open(&db_path).expect("open db");

    repository
        .create_host(host_request(false))
        .expect("create host");
    drop(repository);

    let bytes = fs::read(&db_path).expect("read db bytes");
    let db_text = String::from_utf8_lossy(&bytes);

    assert!(!db_text.contains("super-secret-password"));
}

#[test]
fn host_repository_stores_passwords_in_vault_and_only_persists_references() {
    let db_path = temp_db_path("vault-store");
    let vault = RecordingVault::default();
    let repository =
        HostRepository::open_with_vault(&db_path, Box::new(vault.clone())).expect("open db");

    let created = repository
        .create_host(host_request(true))
        .expect("create host with credential");

    assert_eq!(
        created.credential_ref.as_deref(),
        Some(format!("test-vault://host/{}", created.id).as_str())
    );
    let stored = vault.stored();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].host_id, created.id);
    assert_eq!(stored[0].host_name, "lab-mac");
    assert_eq!(stored[0].password, "super-secret-password");

    drop(repository);
    let bytes = fs::read(&db_path).expect("read db bytes");
    let db_text = String::from_utf8_lossy(&bytes);
    assert!(!db_text.contains("super-secret-password"));
    assert!(db_text.contains("test-vault://host/"));
}

#[test]
fn host_repository_resolves_saved_passwords_for_connection_without_exposing_missing_values() {
    let db_path = temp_db_path("vault-resolve");
    let vault = RecordingVault::default();
    let repository =
        HostRepository::open_with_vault(&db_path, Box::new(vault.clone())).expect("open db");

    let created = repository
        .create_host(host_request(true))
        .expect("create host with credential");
    let password = repository
        .resolve_host_password(&created.id)
        .expect("resolve host password");

    assert_eq!(password.as_deref(), Some("super-secret-password"));

    let plain_host = repository
        .create_host(host_request(false))
        .expect("create host without credential");
    assert_eq!(
        repository
            .resolve_host_password(&plain_host.id)
            .expect("resolve empty credential"),
        None
    );
}

#[test]
fn host_repository_reports_missing_vault_entries_as_errors() {
    let db_path = temp_db_path("vault-missing");
    let vault = RecordingVault::default();
    let repository =
        HostRepository::open_with_vault(&db_path, Box::new(vault.clone())).expect("open db");
    let created = repository
        .create_host(host_request(true))
        .expect("create host with credential");

    vault.stored.lock().expect("stored lock").clear();

    let error = repository
        .resolve_host_password(&created.id)
        .expect_err("missing credential should be an error");

    assert!(error.contains("credential vault read failed"));
    assert!(error.contains("no saved password found"));
}

#[test]
fn host_repository_preserves_existing_credential_when_update_password_is_empty() {
    let db_path = temp_db_path("vault-preserve");
    let vault = RecordingVault::default();
    let repository =
        HostRepository::open_with_vault(&db_path, Box::new(vault.clone())).expect("open db");
    let created = repository
        .create_host(host_request(true))
        .expect("create host");

    let updated = repository
        .update_host(UpdateHostRequest {
            id: created.id.clone(),
            name: "lab-mac-renamed".to_string(),
            address: "127.0.0.2".to_string(),
            port: 5901,
            protocol: "vnc".to_string(),
            group: "ops".to_string(),
            save_password: true,
            password: Some("".to_string()),
        })
        .expect("update host");

    assert_eq!(updated.credential_ref, created.credential_ref);
    assert_eq!(vault.stored().len(), 1);
}

#[test]
fn host_repository_removes_credential_reference_when_password_saving_is_disabled() {
    let db_path = temp_db_path("vault-remove");
    let vault = RecordingVault::default();
    let repository =
        HostRepository::open_with_vault(&db_path, Box::new(vault.clone())).expect("open db");
    let created = repository
        .create_host(host_request(true))
        .expect("create host");
    let credential_ref = created.credential_ref.clone().expect("credential ref");

    let updated = repository
        .update_host(UpdateHostRequest {
            id: created.id,
            name: "lab-mac".to_string(),
            address: "127.0.0.1".to_string(),
            port: 5900,
            protocol: "vnc".to_string(),
            group: "lab".to_string(),
            save_password: false,
            password: None,
        })
        .expect("update host");

    assert_eq!(updated.credential_ref, None);
    assert_eq!(vault.deleted(), vec![credential_ref]);
}

#[test]
fn host_repository_updates_hosts_without_writing_plaintext_passwords() {
    let db_path = temp_db_path("update");
    let repository = HostRepository::open(&db_path).expect("open db");
    let created = repository
        .create_host(host_request(false))
        .expect("create host");

    let updated = repository
        .update_host(UpdateHostRequest {
            id: created.id.clone(),
            name: "lab-mac-renamed".to_string(),
            address: "127.0.0.2".to_string(),
            port: 5901,
            protocol: "vnc".to_string(),
            group: "ops".to_string(),
            save_password: false,
            password: Some("updated-secret-password".to_string()),
        })
        .expect("update host");

    assert_eq!(updated.id, created.id);
    assert_eq!(updated.name, "lab-mac-renamed");
    assert_eq!(updated.address, "127.0.0.2");
    assert_eq!(updated.port, 5901);

    drop(repository);
    let bytes = fs::read(&db_path).expect("read db bytes");
    let db_text = String::from_utf8_lossy(&bytes);
    assert!(!db_text.contains("updated-secret-password"));
}

#[test]
fn host_repository_deletes_hosts() {
    let db_path = temp_db_path("delete");
    let repository = HostRepository::open(&db_path).expect("open db");
    let created = repository
        .create_host(host_request(false))
        .expect("create host");

    repository.delete_host(&created.id).expect("delete host");

    assert!(repository.list_hosts().expect("list hosts").is_empty());
}

#[test]
fn host_repository_deletes_saved_credentials_when_deleting_host() {
    let db_path = temp_db_path("delete-vault");
    let vault = RecordingVault::default();
    let repository =
        HostRepository::open_with_vault(&db_path, Box::new(vault.clone())).expect("open db");
    let created = repository
        .create_host(host_request(true))
        .expect("create host");
    let credential_ref = created.credential_ref.clone().expect("credential ref");

    repository.delete_host(&created.id).expect("delete host");

    assert!(repository.list_hosts().expect("list hosts").is_empty());
    assert_eq!(vault.deleted(), vec![credential_ref]);
}

#[test]
fn host_repository_records_recent_use_before_any_connection_result() {
    let db_path = temp_db_path("connection-attempt");
    let repository = HostRepository::open(&db_path).expect("open db");
    let created = repository
        .create_host(host_request(false))
        .expect("create host");
    assert_eq!(created.last_connected_at, None);

    let attempted = repository
        .record_connection_attempt(&created.id)
        .expect("record connection attempt");
    let attempted_at = attempted.last_connected_at.as_deref().expect("recent use");
    assert!(attempted_at.contains('T') && attempted_at.ends_with('Z'));

    drop(repository);
    let reopened = HostRepository::open(&db_path).expect("reopen db");
    let persisted = reopened.list_hosts().expect("persisted hosts");
    assert_eq!(persisted[0].last_connected_at, attempted.last_connected_at);
    assert!(reopened.record_connection_attempt("missing-host").is_err());
}

#[test]
fn host_repository_creates_and_lists_groups_without_hosts() {
    let db_path = temp_db_path("create-group");
    let repository = HostRepository::open(&db_path).expect("open db");

    let group = repository.create_group(" ops rack ").expect("create group");
    let groups = repository.list_groups().expect("list groups");

    assert_eq!(group.name, "ops rack");
    assert_eq!(group.host_count, 0);
    assert!(groups.iter().any(|item| item.name == "未分组"));
    assert!(groups
        .iter()
        .any(|item| item.name == "ops rack" && item.host_count == 0));
}

#[test]
fn host_repository_backfills_groups_from_created_hosts() {
    let db_path = temp_db_path("host-groups");
    let repository = HostRepository::open(&db_path).expect("open db");
    repository
        .create_host(host_request_with_group("lab"))
        .expect("create first host");
    repository
        .create_host(host_request_with_group("lab"))
        .expect("create second host");

    let groups = repository.list_groups().expect("list groups");

    assert!(groups
        .iter()
        .any(|item| item.name == "lab" && item.host_count == 2));
}

#[test]
fn host_repository_renames_groups_across_hosts() {
    let db_path = temp_db_path("rename-group");
    let repository = HostRepository::open(&db_path).expect("open db");
    repository
        .create_host(host_request_with_group("lab"))
        .expect("create first host");
    repository
        .create_host(host_request_with_group("lab"))
        .expect("create second host");
    repository
        .create_host(host_request_with_group("ops"))
        .expect("create other host");

    let changed = repository
        .rename_group(" lab ", " production ")
        .expect("rename group");
    let hosts = repository.list_hosts().expect("list hosts");
    let groups = repository.list_groups().expect("list groups");

    assert_eq!(changed, 2);
    assert_eq!(
        hosts
            .iter()
            .filter(|host| host.group == "production")
            .count(),
        2
    );
    assert_eq!(hosts.iter().filter(|host| host.group == "ops").count(), 1);
    assert!(groups
        .iter()
        .any(|group| group.name == "production" && group.host_count == 2));
    assert!(!groups.iter().any(|group| group.name == "lab"));
}

#[test]
fn host_repository_deletes_groups_by_moving_hosts_to_default_group() {
    let db_path = temp_db_path("delete-group");
    let repository = HostRepository::open(&db_path).expect("open db");
    repository
        .create_host(host_request_with_group("lab"))
        .expect("create first host");
    repository
        .create_host(host_request_with_group("lab"))
        .expect("create second host");

    let changed = repository.delete_group("lab").expect("delete group");
    let hosts = repository.list_hosts().expect("list hosts");
    let groups = repository.list_groups().expect("list groups");

    assert_eq!(changed, 2);
    assert!(hosts.iter().all(|host| host.group == "未分组"));
    assert!(!groups.iter().any(|group| group.name == "lab"));
    assert!(groups
        .iter()
        .any(|group| group.name == "未分组" && group.host_count == 2));
}
