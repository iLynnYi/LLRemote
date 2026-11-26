import assert from "node:assert/strict";
import test from "node:test";
import { ref } from "vue";
import { createPasswordPromptController } from "../../../src/app/composables/vncAppState/passwordPrompt.ts";
import type { PasswordPromptState } from "../../../src/app/types.ts";

function fixture() {
  const state = ref<PasswordPromptState>({ visible: false, target: "", password: "" });
  const messages: Array<{ content: string; tone: string }> = [];
  return { state, messages, prompt: createPasswordPromptController({
    passwordPrompt: state, t: key => key,
    notify: (content, tone) => { messages.push({ content, tone }); },
  }) };
}

test("an empty submission warns once and keeps the password request open for correction", async () => {
  const f = fixture();
  const requested = f.prompt.requestSessionPassword("target");
  assert.equal(f.prompt.submitPasswordPrompt(), false);
  assert.equal(f.state.value.visible, true);
  assert.deepEqual(f.messages, [{ content: "sessions.passwordPromptEmpty", tone: "warning" }]);
  const password = crypto.randomUUID();
  f.state.value.password = password;
  assert.equal(f.prompt.submitPasswordPrompt(), true);
  assert.equal(await requested, password);
  assert.equal(f.state.value.visible, false);
  assert.equal(f.state.value.password, "");
  f.prompt.submitPasswordPrompt();
  assert.equal(f.messages.length, 1);
});

test("aborting a credential request clears its prompt and cannot submit it later", async () => {
  const f = fixture();
  const cancellation = new AbortController();
  const requested = f.prompt.requestSessionPassword("first", "", cancellation.signal);
  assert.equal(f.state.value.visible, true);
  cancellation.abort();
  assert.equal(f.state.value.visible, false);
  assert.equal(await requested, undefined);
  f.prompt.submitPasswordPrompt();
  assert.deepEqual(f.messages, []);
  assert.equal(await f.prompt.requestSessionPassword("closed", "", cancellation.signal), undefined);
  assert.equal(f.state.value.visible, false);
});

test("an old request's abort cannot cancel its replacement prompt", async () => {
  const f = fixture();
  const old = new AbortController();
  const next = new AbortController();
  const first = f.prompt.requestSessionPassword("first", "", old.signal);
  const second = f.prompt.requestSessionPassword("second", "", next.signal);
  assert.equal(await first, undefined);
  old.abort();
  assert.equal(f.state.value.target, "second");
  assert.equal(f.state.value.visible, true);
  next.abort();
  assert.equal(await second, undefined);
  assert.equal(f.state.value.visible, false);
});

test("explicit dialog cancellation calls its handler once and clears the draft", async () => {
  const f = fixture();
  let cancellations = 0;
  const requested = f.prompt.requestSessionPassword("target", "", undefined, () => { cancellations++; });
  f.state.value.password = crypto.randomUUID();
  f.prompt.cancelPasswordPrompt();
  f.prompt.cancelPasswordPrompt();
  assert.equal(await requested, undefined);
  assert.equal(cancellations, 1);
  assert.equal(f.state.value.password, "");
  assert.equal(f.state.value.visible, false);
});

test("replacement and abort are silent instead of reporting an explicit user cancellation", async () => {
  const f = fixture();
  let cancellations = 0;
  const signal = new AbortController();
  const first = f.prompt.requestSessionPassword("first", "", undefined, () => { cancellations++; });
  const second = f.prompt.requestSessionPassword("second", "", signal.signal, () => { cancellations++; });
  assert.equal(await first, undefined);
  signal.abort();
  assert.equal(await second, undefined);
  assert.equal(cancellations, 0);
});
