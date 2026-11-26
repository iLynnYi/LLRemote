import type {
  SessionExternalWindowToolboxCommandPayload,
  SessionExternalWindowToolboxCommandResultPayload,
} from "../external-window/sessionExternalWindow.ts";

/** Keep the draft until the main window returns this particular send's result. */
export async function sendExternalRemoteMessage(options: {
  sessionId: string;
  text: string;
  listen: (handler: (result: SessionExternalWindowToolboxCommandResultPayload) => void) => Promise<() => void>;
  send: (payload: SessionExternalWindowToolboxCommandPayload) => Promise<void>;
  signal?: AbortSignal;
  timeoutMs?: number;
}): Promise<boolean> {
  const requestId = crypto.randomUUID();
  let finish!: (success: boolean) => void;
  let fail!: (error: Error) => void;
  const result = new Promise<boolean>((resolve, reject) => { finish = resolve; fail = reject; });
  // Observe immediately: cancellation may happen while listener setup is pending.
  void result.catch(() => undefined);
  let stopped = false;
  const stop = (code: string) => { stopped = true; fail(new Error(code)); };
  const abort = () => stop("remote_message_disconnected");
  options.signal?.addEventListener("abort", abort, { once: true });
  let unlisten: (() => void) | undefined;
  const timer = setTimeout(() => stop("remote_message_unconfirmed"), options.timeoutMs ?? 10_000);
  try {
    if (options.signal?.aborted) throw new Error("remote_message_disconnected");
    unlisten = await options.listen((reply) => {
      if (reply.sessionId === options.sessionId && reply.requestId === requestId && typeof reply.succeeded === "boolean") {
        finish(reply.succeeded);
      }
    });
    if (options.signal?.aborted) throw new Error("remote_message_disconnected");
    if (stopped) return await result;
    await options.send({ sessionId: options.sessionId, key: "action:send-message", text: options.text,
      requestId, feedbackTarget: "external" });
    return await result;
  } finally {
    clearTimeout(timer);
    unlisten?.();
    options.signal?.removeEventListener("abort", abort);
  }
}
