export function captureForwardedEvent(event: Event) {
  event.preventDefault();
  event.stopPropagation();
}

export function isInteractiveTarget(event: Event): boolean {
  const selectors = [
    "button",
    "input",
    "select",
    "textarea",
    "[contenteditable='true']",
    "[role='button']",
    ".n-button",
    ".n-dropdown-menu",
    ".n-input",
    ".n-select",
    ".session-toolbox-trigger",
    ".session-toolbox-button",
    ".session-toolbox-menu",
    ".remote-viewport-hud",
    ".remote-viewport-mode-toggle",
  ].join(",");

  return event.composedPath().some((target) => (
    target instanceof Element && Boolean(target.closest(selectors))
  ));
}

export function isKeyboardEditableTarget(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    target.isContentEditable
  );
}
