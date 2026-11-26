export function formatTimestamp(value: number): string {
  if (!Number.isFinite(value) || value <= 0) {
    return "-";
  }

  return new Date(value).toISOString();
}

export function formatTimeOfDay(value: number): string {
  if (!Number.isFinite(value) || value <= 0) {
    return "-";
  }

  return new Date(value).toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}
