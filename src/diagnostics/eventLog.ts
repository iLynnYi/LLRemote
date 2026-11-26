export type EventLogLevel = "info" | "success" | "warning" | "error";
export type DiagnosticEventOrigin = "conversation" | "host" | "system";

export type DiagnosticEventContext = {
  origin?: DiagnosticEventOrigin;
  target?: string;
  hostId?: string | null;
  hostName?: string;
  conversationId?: string | null;
  conversationName?: string;
  eventKey?: string;
  userMessage?: string;
};

export type EventLogEntry = DiagnosticEventContext & {
  id: string;
  time: string;
  level: EventLogLevel;
  source: string;
  message: string;
};

export type EventLogInput = DiagnosticEventContext & {
  level: EventLogLevel;
  source: string;
  message: string;
};

export type AppendEventLogOptions = {
  limit?: number;
  now?: () => Date;
  id?: () => string;
};

export function appendEventLog(
  entries: EventLogEntry[],
  input: EventLogInput,
  options: AppendEventLogOptions = {},
): EventLogEntry[] {
  const limit = options.limit ?? 80;
  const now = options.now?.() ?? new Date();
  const id = options.id?.() ?? crypto.randomUUID();
  const entry: EventLogEntry = {
    id,
    time: formatLocalEventTime(now),
    level: input.level,
    source: input.source,
    message: input.message,
  };

  if (input.origin) entry.origin = input.origin;
  if (input.target) entry.target = input.target;
  if (input.hostId) entry.hostId = input.hostId;
  if (input.hostName) entry.hostName = input.hostName;
  if (input.conversationId) entry.conversationId = input.conversationId;
  if (input.conversationName) entry.conversationName = input.conversationName;
  if (input.eventKey) entry.eventKey = input.eventKey;
  if (input.userMessage) entry.userMessage = input.userMessage;

  return [
    entry,
    ...entries,
  ].slice(0, limit);
}

export function eventsForSession(entries: EventLogEntry[], sessionId: string): EventLogEntry[] {
  return entries.filter((entry) => entry.origin === "conversation" && entry.conversationId === sessionId);
}

function formatLocalEventTime(date: Date): string {
  return [
    date.getHours(),
    date.getMinutes(),
    date.getSeconds(),
  ].map((part) => String(part).padStart(2, "0")).join(":");
}
