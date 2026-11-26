use crate::host_store::types::{HostGroup, HostRecord};
use rusqlite::{params, Connection};

pub(crate) struct NormalizedHostFields {
    pub(crate) name: String,
    pub(crate) address: String,
    pub(crate) port: u16,
    pub(crate) protocol: String,
    pub(crate) group: String,
}
