use crate::host_store;
use tauri::State;

#[tauri::command]
pub fn list_hosts(
    repository: State<'_, host_store::HostRepository>,
) -> Result<Vec<host_store::HostRecord>, String> {
    repository.list_hosts()
}

#[tauri::command]
pub fn create_host(
    repository: State<'_, host_store::HostRepository>,
    request: host_store::CreateHostRequest,
) -> Result<host_store::HostRecord, String> {
    repository.create_host(request)
}

#[tauri::command]
pub fn update_host(
    repository: State<'_, host_store::HostRepository>,
    request: host_store::UpdateHostRequest,
) -> Result<host_store::HostRecord, String> {
    repository.update_host(request)
}

#[tauri::command]
pub fn delete_host(
    repository: State<'_, host_store::HostRepository>,
    id: String,
) -> Result<(), String> {
    repository.delete_host(&id)
}

#[tauri::command]
pub fn record_host_connection_attempt(
    repository: State<'_, host_store::HostRepository>,
    id: String,
) -> Result<host_store::HostRecord, String> {
    repository.record_connection_attempt(&id)
}

#[tauri::command]
pub fn list_host_groups(
    repository: State<'_, host_store::HostRepository>,
) -> Result<Vec<host_store::HostGroup>, String> {
    repository.list_groups()
}

#[tauri::command]
pub fn create_host_group(
    repository: State<'_, host_store::HostRepository>,
    request: host_store::CreateGroupRequest,
) -> Result<host_store::HostGroup, String> {
    repository.create_group(&request.name)
}

#[tauri::command]
pub fn resolve_host_password(
    repository: State<'_, host_store::HostRepository>,
    id: String,
) -> Result<Option<String>, String> {
    repository.resolve_host_password(&id)
}

#[tauri::command]
pub fn rename_host_group(
    repository: State<'_, host_store::HostRepository>,
    request: host_store::RenameGroupRequest,
) -> Result<usize, String> {
    repository.rename_group(&request.from, &request.to)
}

#[tauri::command]
pub fn delete_host_group(
    repository: State<'_, host_store::HostRepository>,
    request: host_store::DeleteGroupRequest,
) -> Result<usize, String> {
    repository.delete_group(&request.group)
}
