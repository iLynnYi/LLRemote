import type { ClipboardAccess } from "../clipboard/clipboardTypes.ts";

export type MessageDraftEditAction = "copy" | "cut" | "paste" | "selectAll";

export function messageDraftEditShortcut(event: Pick<KeyboardEvent,
  "key" | "code" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey" | "isComposing"
>): MessageDraftEditAction | null {
  if (event.isComposing || event.altKey || event.ctrlKey === event.metaKey) return null;
  const key = /^Key[A-Z]$/.test(event.code) ? event.code.slice(3).toLowerCase() : event.key.toLowerCase();
  if (event.shiftKey && key !== "v") return null;
  return ({ c: "copy", x: "cut", v: "paste", a: "selectAll" } as const)[key as "c" | "x" | "v" | "a"] ?? null;
}

export type MessageDraftEditor = {
  readonly value: string;
  readonly selectionStart: number;
  readonly selectionEnd: number;
  replaceSelection: (text: string) => void;
  selectAll: () => void;
};

/** Only edit the local draft; never dispatch a clipboard/keyboard action remotely. */
export async function editMessageDraftClipboard(
  action: MessageDraftEditAction,
  editor: MessageDraftEditor,
  clipboard: ClipboardAccess | undefined,
  stillActive: () => boolean,
): Promise<void> {
  if (!stillActive()) return;
  if (action === "selectAll") { editor.selectAll(); return; }
  const value = editor.value;
  const start = editor.selectionStart;
  const end = editor.selectionEnd;
  const unchanged = () => stillActive() && editor.value === value
    && editor.selectionStart === start && editor.selectionEnd === end;
  if (action === "copy" || action === "cut") {
    // An empty selection must not overwrite the user's existing clipboard.
    if (start === end) return;
    if (!clipboard) throw new Error("clipboard_unavailable");
    await clipboard.writeText(value.slice(start, end));
    if (action === "cut" && unchanged()) editor.replaceSelection("");
    return;
  }
  if (!clipboard?.readText) throw new Error("clipboard_unavailable");
  const text = await clipboard.readText();
  // A late read must not modify a different draft or a selection moved meanwhile.
  if (text && unchanged()) editor.replaceSelection(text);
}
