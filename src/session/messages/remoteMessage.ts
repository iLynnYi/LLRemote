export const MAX_REMOTE_MESSAGE_BYTES = 16 * 1024;

export function remoteMessageTextError(text: string): string | null {
  if (!text.trim()) return "remote_message_empty";
  if (text.includes("\0")) return "remote_message_invalid";
  if (new TextEncoder().encode(text).length > MAX_REMOTE_MESSAGE_BYTES) return "remote_message_too_large";
  return null;
}
