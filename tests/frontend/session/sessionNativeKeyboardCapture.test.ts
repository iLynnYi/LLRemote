import assert from "node:assert/strict";
import test from "node:test";
import { createNativeKeyboardCaptureController } from "../../../src/session/input/sessionNativeKeyboardCapture.ts";

function fixture(setCapture: (enabled: boolean, token: string) => Promise<boolean> = async () => true) {
  let sequence = 0;
  const commands: Array<[boolean, string]> = [];
  const released: string[] = [];
  const keys: string[] = [];
  let unavailable = 0;
  let stopped = 0;
  const states: string[] = [];
  const controller = createNativeKeyboardCaptureController({
    setCapture: (enabled, token) => { commands.push([enabled, token]); return setCapture(enabled, token); },
    token: () => String(++sequence),
    onUnavailable: () => unavailable++,
    onStopped: () => stopped++,
    onStateChanged: (status: string) => states.push(status),
  });
  const target = (id: string) => ({ id, release: () => released.push(id), onKey: () => keys.push(id) });
  return { controller, commands, released, keys, target, states, unavailable: () => unavailable, stopped: () => stopped };
}

test("native events cannot cross session ownership, even while enable is pending", async () => {
  let finish!: (available: boolean) => void;
  const f = fixture(async (_enabled, token) => token === "1" && _enabled ? new Promise(resolve => { finish = resolve; }) : true);
  f.controller.setTarget(f.target("first"));
  await Promise.resolve();
  f.controller.setTarget(f.target("second"));
  f.controller.consume({ token: "1", key: { key: "Meta", down: true }, reason: null });
  finish(true);
  await f.controller.settled();
  assert.deepEqual(f.commands, [[true, "1"], [false, "1"], [true, "2"]]);
  assert.deepEqual(f.released, ["first"]);
  assert.deepEqual(f.keys, []);
  f.controller.consume({ token: "2", key: { key: "Meta", down: true }, reason: null });
  assert.deepEqual(f.keys, ["second"]);
  f.controller.dispose();
  await f.controller.settled();
  assert.deepEqual(f.released, ["first", "second"]);
  assert.deepEqual(f.commands.at(-1), [false, "2"]);
});

test("native focus loss and overload release input until the next focus entry", async () => {
  for (const reason of ["focus", "overflow"] as const) {
    const f = fixture();
    f.controller.setTarget(f.target("session"));
    await f.controller.settled();
    f.controller.consume({ token: "1", key: null, reason });
    f.controller.setTarget(f.target("session"));
    await f.controller.settled();
    assert.equal(f.stopped(), reason === "overflow" ? 0 : 1);
    assert.equal(f.unavailable(), reason === "overflow" ? 1 : 0);
    assert.deepEqual(f.commands, [[true, "1"], [false, "1"]]);
    assert.deepEqual(f.released, ["session"]);
    f.controller.resume();
    f.controller.setTarget(f.target("session"));
    await f.controller.settled();
    assert.deepEqual(f.commands.at(-1), [true, "2"]);
    f.controller.dispose();
  }
});

test("native failure keeps ordinary input available without an automatic retry loop", async () => {
  const f = fixture(async enabled => !enabled);
  f.controller.setTarget(f.target("session"));
  await f.controller.settled();
  await f.controller.settled();
  f.controller.setTarget(f.target("session"));
  assert.equal(f.unavailable(), 1);
  assert.deepEqual(f.released, ["session"]);
  assert.deepEqual(f.commands, [[true, "1"], [false, "1"]]);
});

test("teardown before pending installation completes disables that exact owner", async () => {
  let finish!: (available: boolean) => void;
  const f = fixture(async enabled => enabled ? new Promise(resolve => { finish = resolve; }) : false);
  f.controller.setTarget(f.target("session"));
  await Promise.resolve();
  f.controller.dispose();
  finish(true);
  await f.controller.settled();
  f.controller.consume({ token: "1", key: { key: "a", down: true }, reason: null });
  assert.deepEqual(f.commands, [[true, "1"], [false, "1"]]);
  assert.deepEqual(f.keys, []);
});

test("disabled macOS tap releases input and reports failure without reinstallation loops", async () => {
  const f = fixture();
  f.controller.setTarget(f.target("session"));
  await f.controller.settled();
  f.controller.consume({token: "1", key: null, reason: "unavailable"});
  f.controller.setTarget(f.target("session"));
  await f.controller.settled();
  assert.equal(f.unavailable(), 1);
  assert.deepEqual(f.released, ["session"]);
  assert.deepEqual(f.commands, [[true, "1"], [false, "1"]]);
  f.controller.consume({token: "1", key: {key: "a", down: true}, reason: null});
  assert.deepEqual(f.keys, []);
});


test("readiness follows the installation acknowledgement and survives keyboard idle", async () => {
  let acknowledge!: (ready: boolean) => void;
  const f = fixture(enabled => enabled ? new Promise(resolve => { acknowledge = resolve; }) : Promise.resolve(false));
  f.controller.setTarget(f.target("session"));
  assert.equal(f.states.at(-1), "requested");
  await Promise.resolve();
  acknowledge(true);
  await f.controller.settled();
  assert.equal(f.states.at(-1), "ready");
  // No keyboard message is needed to keep a live, installed capture ready.
  await f.controller.settled();
  assert.deepEqual(f.states, ["requested", "ready"]);
  f.controller.dispose();
  await f.controller.settled();
  assert.equal(f.states.at(-1), "inactive");
});

test("worker failure releases held input and stays unavailable until explicit focus recovery", async () => {
  const f = fixture();
  f.controller.setTarget(f.target("session"));
  await f.controller.settled();
  f.controller.consume({token:"1", key:{key:"Meta",down:true}, reason:null});
  f.controller.consume({token:"1", key:null, reason:"unavailable"});
  await f.controller.settled();
  assert.equal(f.states.at(-1), "unavailable");
  assert.deepEqual(f.released, ["session"]);
  f.controller.setTarget(f.target("session"));
  await f.controller.settled();
  assert.equal(f.commands.filter(([enabled]) => enabled).length, 1);
  f.controller.resume();
  f.controller.setTarget(f.target("session"));
  await f.controller.settled();
  assert.equal(f.states.at(-1), "ready");
  assert.deepEqual(f.commands.at(-1), [true,"2"]);
  f.controller.consume({token:"1", key:{key:"c",down:true}, reason:null});
  assert.deepEqual(f.keys, ["session"]);
  f.controller.dispose();
});

test("a terminal message during installation cannot be overwritten by a late ready acknowledgement", async () => {
  let acknowledge!: (ready: boolean) => void;
  const f = fixture(enabled => enabled ? new Promise(resolve => { acknowledge = resolve; }) : Promise.resolve(false));
  f.controller.setTarget(f.target("session"));
  await Promise.resolve();
  f.controller.consume({token:"1", key:null, reason:"unavailable"});
  acknowledge(true);
  await f.controller.settled();
  assert.equal(f.states.at(-1), "unavailable");
  assert.deepEqual(f.released, ["session"]);
  f.controller.dispose();
});
