export function checkboxValue(event: Event): boolean {
  return event.target instanceof HTMLInputElement ? event.target.checked : false;
}

export function isTauriUnavailableError(error: unknown): boolean {
  const message = String(error).toLowerCase();
  return message.includes("__tauri") || message.includes("tauri") || message.includes("ipc") || message.includes("not available");
}

export function setElementTarget(targets: Map<string, HTMLElement>, id: string, element: Element | null) {
  if (element instanceof HTMLElement) {
    targets.set(id, element);
    return;
  }

  targets.delete(id);
}
