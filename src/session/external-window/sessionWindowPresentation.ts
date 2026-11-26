export type SessionWindowState = {
  isFullscreen: () => Promise<boolean>;
  isMaximized: () => Promise<boolean>;
  setFullscreen: (fullscreen: boolean) => Promise<void>;
  maximize: () => Promise<void>;
  unmaximize: () => Promise<void>;
};

/** Fill the monitor work area on both platforms, preserving taskbar/Dock and window controls. */
export function nativeSessionWindowOptions(expanded: boolean) {
  return { fullscreen: false, maximized: expanded };
}

export async function isSessionWindowExpanded(window: SessionWindowState): Promise<boolean> {
  return await window.isFullscreen() || await window.isMaximized();
}

export async function setSessionWindowExpanded(window: SessionWindowState, expanded: boolean): Promise<void> {
  // Also leave native fullscreen if the OS green button or an older window entered it.
  if (await window.isFullscreen()) await window.setFullscreen(false);
  return expanded ? window.maximize() : window.unmaximize();
}
