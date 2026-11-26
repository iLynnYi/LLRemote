mod credentials;
mod groups;
pub(crate) mod types;

use crate::credential_vault::{CredentialSecret, CredentialVault, SystemCredentialVault};
use crate::host_store_storage::{
    current_timestamp, initialize_schema, normalize_host_fields, read_host_row, upsert_group,
    validate_create_password_storage,
};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;
pub use types::{
    CreateGroupRequest, CreateHostRequest, DeleteGroupRequest, HostGroup, HostRecord,
    RenameGroupRequest, UpdateHostRequest,
};
use uuid::Uuid;

include!("repository.rs");
include!("hosts.rs");
include!("recent.rs");
include!("queries.rs");
include!("credentials_update.rs");
