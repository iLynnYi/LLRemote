/** Native commands may reject with either a string code or an Error with OS details. */
function errorCode(error: unknown): string {
  return (error instanceof Error ? error.message : String(error)).split(":")[0]?.trim() ?? "";
}

export function isFileTransferConflictSkipped(error: unknown): boolean {
  return errorCode(error) === "agent_file_conflict_skipped";
}

const ERROR_GROUPS: Record<string, readonly string[]> = {
  agent_file_conflict_skipped: ["agent_file_conflict_skipped"],
  conflict: ["agent_file_conflict_requires_choice"],
  agent_file_too_large: ["agent_file_too_large"],
  agent_file_directory_unsupported: ["agent_file_directory_unsupported"],
  agent_file_relative_path_invalid: ["agent_file_relative_path_invalid", "agent_file_name_invalid", "agent_target_directory_invalid"],
  agent_file_read_failed: ["agent_file_read_failed", "agent_file_seek_failed"],
  agent_file_create_failed: ["agent_file_create_failed", "agent_file_create_missing_transfer_id"],
  agent_file_chunk_failed: ["agent_file_chunk_failed"],
  agent_file_commit_failed: ["agent_file_commit_failed", "agent_file_commit_rename_failed", "agent_file_overwrite_failed"],
  remoteWrite: ["agent_inbox_create_failed", "agent_file_part_create_failed", "agent_file_part_open_failed", "agent_file_part_stat_failed", "agent_file_part_write_failed"],
  validation: ["agent_file_checksum_mismatch", "agent_file_size_mismatch", "agent_file_offset_mismatch", "agent_file_chunk_decode_failed", "agent_file_chunk_exceeds_size", "agent_file_hash_open_failed", "agent_file_hash_read_failed"],
  authentication: ["agent_auth_failed", "agent_authentication_failed", "agent_authentication_required", "agent_session_invalid", "agent_session_required", "agent_ticket_expired"],
  agent_unreachable: ["agent_unreachable", "agent_address_resolve_empty", "agent_address_resolve_failed"],
  connection: ["agent_channel_unavailable", "agent_endpoint_unavailable", "agent_session_open_failed", "agent_session_describe_failed", "agent_response_empty"],
  timeout: ["agent_response_timeout", "agent_read_timeout_failed", "agent_write_timeout_failed"],
  permission: ["agent_view_only", "agent_user_desktop_unavailable"],
  unsupported: ["agent_unsupported_request"],
  unavailable: ["file_transfer_restore_unavailable", "agent_transfer_not_found", "agent_transfer_fingerprint_mismatch"],
  settings: ["agent_rename_policy_invalid"],
  paused: ["agent_file_paused"],
  agent_request_failed: ["agent_request_failed", "agent_request_decode_failed", "agent_request_encode_failed", "agent_request_flush_failed", "agent_request_read_failed", "agent_request_write_failed", "agent_request_too_large", "agent_response_decode_failed", "agent_response_encode_failed", "agent_response_read_failed", "agent_session_state_poisoned", "agent_transfer_state_poisoned", "agent_transfer_id_required", "agent_file_upload_worker_failed"],
};

/** Only translation keys reach the UI; retain raw details on the task for diagnostics. */
export function fileTransferErrorMessageKey(error: unknown): string {
  const code = errorCode(error);
  const group = Object.entries(ERROR_GROUPS).find(([, codes]) => codes.includes(code))?.[0] ?? "unknown";
  return `sessions.fileTransfers.errors.${group}`;
}
