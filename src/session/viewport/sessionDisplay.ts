export type SessionQualityProfile = "balanced" | "sharp" | "fast";

export const SESSION_QUALITY_PROFILES: SessionQualityProfile[] = ["balanced", "sharp", "fast"];

export function isSessionQualityProfile(value: unknown): value is SessionQualityProfile {
  return SESSION_QUALITY_PROFILES.includes(value as SessionQualityProfile);
}

export type SessionDisplayInfo = {
  width: number;
  height: number;
  aspectRatio: number;
  multiDisplayHint: boolean;
};

export type SessionDisplaySummary = SessionDisplayInfo & {
  label: string;
  detail: string;
};

export function summarizeSessionDisplay(info: SessionDisplayInfo | null): SessionDisplaySummary {
  if (!info || info.width <= 0 || info.height <= 0) {
    return {
      width: 0,
      height: 0,
      aspectRatio: 0,
      multiDisplayHint: false,
      label: "sessions.display.unknown",
      detail: "sessions.display.unknownDetail",
    };
  }

  return {
    ...info,
    label: info.multiDisplayHint ? "sessions.display.multiHint" : "sessions.display.singleHint",
    detail: info.multiDisplayHint ? "sessions.display.multiHintDetail" : "sessions.display.singleHintDetail",
  };
}

export function createSessionDisplayInfo(width: number, height: number): SessionDisplayInfo {
  const safeWidth = Math.max(0, Math.round(width));
  const safeHeight = Math.max(0, Math.round(height));
  const aspectRatio = safeWidth > 0 && safeHeight > 0 ? safeWidth / safeHeight : 0;

  return {
    width: safeWidth,
    height: safeHeight,
    aspectRatio,
    multiDisplayHint: aspectRatio >= 2.4,
  };
}

type SessionDisplayInventoryItem = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export function createSessionDisplayInfoFromAgentDisplays(displays: SessionDisplayInventoryItem[]): SessionDisplayInfo {
  if (!displays.length) {
    return createSessionDisplayInfo(0, 0);
  }

  const left = Math.min(...displays.map((display) => display.x));
  const top = Math.min(...displays.map((display) => display.y));
  const right = Math.max(...displays.map((display) => display.x + display.width));
  const bottom = Math.max(...displays.map((display) => display.y + display.height));
  return {
    ...createSessionDisplayInfo(right - left, bottom - top),
    multiDisplayHint: displays.length > 1,
  };
}

export function mergeSessionDisplayInfo(
  current: SessionDisplayInfo | null | undefined,
  next: SessionDisplayInfo,
  preserveCurrentMultiDisplayHint = false,
): SessionDisplayInfo {
  if (!preserveCurrentMultiDisplayHint || !current?.multiDisplayHint || next.multiDisplayHint) {
    return next;
  }

  return {
    ...next,
    multiDisplayHint: true,
  };
}
