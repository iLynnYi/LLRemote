import type { EventLogLevel } from "./eventLog.ts";

type RuntimeLogLevel = "error" | "warn" | "info" | "debug" | "trace";

type RuntimeLogEvent = {
  level: RuntimeLogLevel;
  component: string;
  event: string;
  message: string;
  summary?: string;
  source?: string;
  target?: string;
  sessionId?: string;
  attemptId?: string;
};

type Invoke = (command: "append_frontend_log_batch", args: { events: RuntimeLogEvent[] }) => Promise<unknown>;

const MAX_PENDING_EVENTS = 200;
const FLUSH_INTERVAL_MS = 300;

export function createRuntimeFileLogger(invoke: Invoke) {
  const pending: RuntimeLogEvent[] = [];
  let timer: ReturnType<typeof window.setTimeout> | undefined;
  let flushing = false;

  function emit(level: EventLogLevel | RuntimeLogLevel, component: string, event: string, message: string, context: Omit<RuntimeLogEvent, "level" | "component" | "event" | "message"> = {}) {
    const normalizedLevel = normalizeLevel(level);
    if (pending.length >= MAX_PENDING_EVENTS && (normalizedLevel === "debug" || normalizedLevel === "trace")) return;
    if (pending.length >= MAX_PENDING_EVENTS) pending.shift();
    pending.push({ level: normalizedLevel, component, event, message, ...context });
    if (normalizedLevel === "error" || normalizedLevel === "warn" || pending.length >= 20) {
      void flush();
      return;
    }
    timer ??= window.setTimeout(() => { timer = undefined; void flush(); }, FLUSH_INTERVAL_MS);
  }

  async function flush() {
    if (flushing || !pending.length) return;
    flushing = true;
    const events = pending.splice(0, 20);
    try {
      await invoke("append_frontend_log_batch", { events });
    } catch {
      // Logging must never block or recursively report a VNC lifecycle event.
    } finally {
      flushing = false;
      if (pending.length) void flush();
    }
  }

  return { emit, flush };
}

function normalizeLevel(level: EventLogLevel | RuntimeLogLevel): RuntimeLogLevel {
  if (level === "success") return "info";
  if (level === "warning") return "warn";
  return level;
}

export function summarizePerformanceLog(source: string, message: string): string | undefined {
  if (source !== "rfb-performance" && source !== "surface-performance") return undefined;
  const fields = Object.fromEntries([...message.matchAll(/\b([a-z_]+)=([^\s]+)/g)].map((match) => [match[1], match[2]]));
  if (source === "surface-performance") {
    return `画面性能：${fields.fps ?? "?"} 帧/秒，${fields.presented_frames ?? "?"} 帧，最长间隔 ${fields.max_interval_ms ?? "?"} 毫秒`;
  }
  return `连接性能：${fields.updates ?? "?"} 次更新，${fields.committed_frames ?? "?"} 帧，接收 ${fields.received_bytes ?? "?"} 字节`;
}
