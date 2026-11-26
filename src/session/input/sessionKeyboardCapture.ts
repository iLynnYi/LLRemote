export type SessionKeyboardCaptureInput = {
  readonly: boolean;
  active: boolean;
  visible: boolean;
  editableTarget: boolean;
  reservedShortcut: boolean;
};

export function shouldCaptureSessionKeyboardEvent(input: SessionKeyboardCaptureInput): boolean {
  return !input.readonly
    && input.active
    && input.visible
    && !input.editableTarget
    && !input.reservedShortcut;
}
