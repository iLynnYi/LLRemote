/** Display timestamps in the client's local timezone; stored timestamps stay in UTC. */
export function formatArtifactCreatedAt(createdAt: string, nowMs: number, locale?: string) {
  const date = new Date(createdAt);
  const now = new Date(nowMs);
  if (!Number.isFinite(date.getTime())) return { isToday: false, date: "-", time: "", full: "-" };
  return {
    isToday: date.toDateString() === now.toDateString(),
    date: date.toLocaleDateString(locale, {
      ...(date.getFullYear() !== now.getFullYear() ? { year: "numeric" as const } : {}),
      month: "long", day: "numeric",
    }),
    time: date.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false }),
    full: date.toLocaleString(locale, {
      year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit",
      hour12: false, timeZoneName: "short",
    }),
  };
}

export function artifactDirectoryName(path: string): string {
  if (/^(?:[a-zA-Z]:)?[\\/]+$/.test(path.trim())) return path.trim();
  const trimmed = path.trim().replace(/[\\/]+$/, "");
  return trimmed.split(/[\\/]/).pop() || path.trim();
}

export function formatArtifactDuration(seconds: number): string {
  const total = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  const parts = [Math.floor(total / 60) % 60, total % 60];
  if (total >= 3600) parts.unshift(Math.floor(total / 3600));
  return parts.map(value => String(value).padStart(2, "0")).join(":");
}
