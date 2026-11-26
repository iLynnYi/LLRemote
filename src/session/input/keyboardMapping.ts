export type RemoteKeyEvent = {
  keysym: number;
  code: string;
};

export type RemoteKeyDispatch = RemoteKeyEvent & {
  down: boolean;
};

/** Freeze the mapping from keydown through keyup, including repeats. */
export function createRemoteKeyboardMapper() {
  const held = new Map<string, RemoteKeyEvent>();
  return {
    map(event: KeyboardEvent, down: boolean, options: { rightControlAsCommand?: boolean } = {}): RemoteKeyEvent | null {
      const id = event.code || `${event.key}:${event.location}`;
      // Ownership loss already released these keys. A later physical keyup
      // must not release a modifier in a replacement session/window.
      if (!down && !held.has(id)) return null;
      const key = held.get(id) ?? keyboardEventToRemoteKeyEvent(event, options);
      if (key && down) held.set(id, key);
      else if (!down) held.delete(id);
      return key;
    },
    clear: () => held.clear(),
  };
}

const SPECIAL_KEYSYMS: Record<string, number> = {
  Backspace: 0xff08,
  Tab: 0xff09,
  Enter: 0xff0d,
  Escape: 0xff1b,
  PrintScreen: 0xff61,
  ScrollLock: 0xff14,
  Pause: 0xff13,
  Insert: 0xff63,
  Delete: 0xffff,
  Home: 0xff50,
  End: 0xff57,
  PageUp: 0xff55,
  PageDown: 0xff56,
  ArrowLeft: 0xff51,
  ArrowUp: 0xff52,
  ArrowRight: 0xff53,
  ArrowDown: 0xff54,
  Shift: 0xffe1,
  Control: 0xffe3,
  Alt: 0xffe9,
  Meta: 0xffeb,
  CapsLock: 0xffe5,
  NumLock: 0xff7f,
  ContextMenu: 0xff67,
};

const RIGHT_MODIFIER_KEYSYMS: Record<string, number> = {
  Shift: 0xffe2,
  Control: 0xffe4,
  Alt: 0xffea,
  Meta: 0xffe8,
};

function keysymForPrintableKey(key: string): number | null {
  const codePoint = key.codePointAt(0);
  if (!codePoint) {
    return null;
  }

  if (codePoint <= 0xff) {
    return codePoint;
  }

  return 0x01000000 + codePoint;
}

export function keyboardEventToRemoteKeyEvent(
  event: KeyboardEvent,
  options: { rightControlAsCommand?: boolean } = {},
): RemoteKeyEvent | null {
  const key = event.key;
  let code = event.code || key;

  if (options.rightControlAsCommand && key === "Control"
    && (code === "ControlRight" || event.location === 2)) {
    return { keysym: RIGHT_MODIFIER_KEYSYMS.Meta, code: "MetaRight" };
  }

  if (!key || key === "Unidentified" || key === "Dead" || key === "Process") {
    return null;
  }

  const functionKey = /^F([1-9]|1\d|2[0-4])$/.exec(key);
  if (functionKey) {
    return {
      keysym: 0xffbd + Number(functionKey[1]),
      code,
    };
  }

  const special = (event.location === 2 || code.endsWith("Right"))
    ? RIGHT_MODIFIER_KEYSYMS[key] ?? SPECIAL_KEYSYMS[key]
    : SPECIAL_KEYSYMS[key];
  if (special) {
    return {
      keysym: special,
      code,
    };
  }

  if ([...key].length === 1) {
    const keysym = keysymForPrintableKey(key);
    return keysym === null ? null : { keysym, code };
  }

  return null;
}
