import assert from "node:assert/strict";
import test from "node:test";
import { createConnectionIssueHandler, createRawConnectionErrorHandler } from "../../../src/features/sessions/composables/sessionConnectionLifecycle/connectionIssues.ts";
import { createSessionManager } from "../../../src/session/core/sessionManager.ts";
import { createConnectionIssue, createDisconnectIssue } from "../../../src/session/diagnostics/sessionDiagnostics.ts";

test("connection issue handling records a typed localized failure and ignores follow-up disconnect noise", () => {
  const manager = createSessionManager({ id: () => "issue-session" });
  const session = manager.create({ protocol: "vnc", host: "127.0.0.1", port: 5900, qualityProfile: "balanced" });
  const messages: string[] = [], errorMessage = { value: "" };
  let synced = 0;
  const apply = createConnectionIssueHandler({
    errorMessage, findSession: id => manager.sessions.find(entry => entry.id === id),
    localizeConnectionIssue: issue => ({ ...issue, message: `localized:${issue.code}` }),
    logEvent() {}, notify: message => messages.push(message), sessionManager: manager,
    syncSessionAfterIssue: () => { synced += 1; }, t: key => key,
  });
  const issue = createConnectionIssue({ reason: "authentication_failed", phase: "failed" });
  assert.equal(issue.code, "authentication_failed");
  apply(session.id, issue);
  assert.equal(manager.sessions[0].lastIssue?.code, issue.code);
  assert.equal(errorMessage.value, `localized:${issue.code}`);
  apply(session.id, createDisconnectIssue("remote_closed"));
  assert.equal(messages.length, 1);
  assert.equal(synced, 1);
  assert.equal(manager.sessions[0].lastIssue?.code, issue.code);
});

test("raw errors cross the same structured issue entry point", () => {
  const errors: unknown[] = [];
  createRawConnectionErrorHandler((session, issue) => errors.push([session, issue.code, issue.phase]))("one", "connection refused");
  assert.equal(errors.length, 1);
  assert.ok(Array.isArray(errors[0]) && errors[0][0] === "one" && errors[0][2] === "failed");
});
