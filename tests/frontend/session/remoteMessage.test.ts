import assert from "node:assert/strict";
import test from "node:test";
import { MAX_REMOTE_MESSAGE_BYTES, remoteMessageTextError } from "../../../src/session/messages/remoteMessage.ts";
import { sendExternalRemoteMessage } from "../../../src/session/messages/externalRemoteMessage.ts";
import type { SessionExternalWindowToolboxCommandResultPayload } from "../../../src/session/external-window/sessionExternalWindow.ts";

test("messages preserve plain multiline text and bound UTF-8 bytes", () => {
  assert.equal(remoteMessageTextError("你好\n<script>plain text</script> 🐱"), null);
  assert.equal(remoteMessageTextError(" \n\t"), "remote_message_empty");
  assert.equal(remoteMessageTextError("hi\0there"), "remote_message_invalid");
  assert.equal(remoteMessageTextError("a".repeat(MAX_REMOTE_MESSAGE_BYTES)), null);
  assert.equal(remoteMessageTextError("你".repeat(Math.floor(MAX_REMOTE_MESSAGE_BYTES / 3) + 1)), "remote_message_too_large");
});

test("external sends wait for their own reply and distinguish failure from delivery", async () => {
  for (const succeeded of [true, false]) {
    let handler!: (reply: SessionExternalWindowToolboxCommandResultPayload) => void;
    let released = 0;
    let settled = false;
    const sending = sendExternalRemoteMessage({
      sessionId: "session-1", text: "hello",
      listen: async (callback) => { handler = callback; return () => { released += 1; }; },
      send: async (payload) => {
        assert.equal(payload.text, "hello");
        assert.equal(payload.key, "action:send-message");
        handler({ sessionId: "other-session", requestId: payload.requestId, succeeded: true, tone: "success", message: "" });
        handler({ sessionId: "session-1", requestId: "old-request", succeeded: true, tone: "success", message: "" });
        await new Promise<void>((resolve) => setImmediate(resolve));
        assert.equal(settled, false);
        handler({ sessionId: "session-1", requestId: payload.requestId, succeeded, tone: succeeded ? "success" : "error", message: "" });
      },
    }).then((result) => { settled = true; return result; });
    assert.equal(await sending, succeeded);
    assert.equal(released, 1);
  }
});

test("timeout or abort during listener setup prevents a late send and releases the listener", async () => {
  for (const abort of [false, true]) {
    let register!: (unlisten: () => void) => void;
    let released = 0;
    let sent = 0;
    const controller = new AbortController();
    const sending = sendExternalRemoteMessage({
      sessionId: "session-1", text: "hello", signal: controller.signal, timeoutMs: 5,
      listen: () => new Promise((resolve) => { register = resolve; }),
      send: async () => { sent += 1; },
    });
    if (abort) controller.abort();
    else await new Promise((resolve) => setTimeout(resolve, 15));
    const rejected = assert.rejects(sending, abort ? /disconnected/ : /unconfirmed/);
    register(() => { released += 1; });
    await rejected;
    assert.equal(sent, 0);
    assert.equal(released, 1);
  }
});

test("a failed event send releases its result listener and reports no success", async () => {
  let released = 0;
  await assert.rejects(sendExternalRemoteMessage({
    sessionId: "session-1", text: "hello",
    listen: async () => () => { released += 1; },
    send: async () => { throw new Error("event failed"); },
  }), /event failed/);
  assert.equal(released, 1);
});
