export type ClipboardAccess = {
  readText?: () => Promise<string>;
  writeText: (text: string) => Promise<void>;
};

export type LocalClipboardSyncSource = "clipboard" | "paste" | "pasteShortcut";
export type LocalClipboardSyncReason = "sent" | "local-to-remote-disabled" | "inactive" | "empty";
