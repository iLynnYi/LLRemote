import assert from "node:assert/strict";
import test from "node:test";
import { createViewportEdgeScrollController, isViewportContentPoint } from "../../../src/session/viewport/viewportScroll.ts";

function fixture() {
  const target = {
    clientLeft: 1, clientTop: 1, clientWidth: 800, clientHeight: 600,
    scrollWidth: 1600, scrollHeight: 1200, scrollLeft: 300, scrollTop: 200,
    getBoundingClientRect: () => ({ left: 10, top: 20 }),
  };
  let enabled = true;
  let nextId = 0;
  let moves = 0;
  const frames = new Map<number, FrameRequestCallback>();
  const controller = createViewportEdgeScrollController({
    getTarget: () => target,
    canScroll: () => enabled,
    requestFrame: callback => { frames.set(++nextId, callback); return nextId; },
    cancelFrame: id => { frames.delete(id); },
    onScroll: () => { moves++; },
  });
  const move = (clientX: number, clientY: number, extra = {}) => controller.move({
    clientX, clientY, pointerType: "mouse", altKey: false, buttons: 0, ...extra,
  });
  const tick = (time = 0) => {
    const scheduled = [...frames.values()];
    frames.clear();
    scheduled.forEach(callback => callback(time));
  };
  return { target, controller, frames, move, tick, disable: () => { enabled = false; }, moves: () => moves };
}

test("content hit testing excludes borders, scrollbars and outside points", () => {
  const { target } = fixture();
  assert.equal(isViewportContentPoint(target, { clientX: 11, clientY: 21 }), true);
  assert.equal(isViewportContentPoint(target, { clientX: 811, clientY: 200 }), false);
  assert.equal(isViewportContentPoint(target, { clientX: 100, clientY: 621 }), false);
  assert.equal(isViewportContentPoint(target, { clientX: 10, clientY: 20 }), false);
  assert.equal(isViewportContentPoint(null, { clientX: 100, clientY: 100 }), false);
});

test("hover scrolls in all four directions and diagonally without a pressed button", () => {
  for (const [x, y, dx, dy] of [[12, 300, -1, 0], [810, 300, 1, 0], [400, 22, 0, -1], [400, 620, 0, 1], [810, 620, 1, 1]]) {
    const f = fixture();
    f.move(x, y);
    f.tick();
    assert.equal(Math.sign(f.target.scrollLeft - 300), dx);
    assert.equal(Math.sign(f.target.scrollTop - 200), dy);
    assert.equal(f.moves(), 1);
    f.controller.stop();
    assert.equal(f.frames.size, 0);
  }
});

test("stationary hover continues scrolling, closer to the edge is faster", () => {
  const near = fixture();
  const far = fixture();
  near.move(810, 300);
  far.move(790, 300);
  near.tick(); far.tick();
  assert.ok(near.target.scrollLeft > far.target.scrollLeft);
  const previous = near.target.scrollLeft;
  near.tick(16);
  assert.ok(near.target.scrollLeft > previous);
});

test("center, outside, touch and local pan gestures stop scheduled scrolling", () => {
  for (const stopMove of [
    (f: ReturnType<typeof fixture>) => f.move(400, 300),
    (f: ReturnType<typeof fixture>) => f.move(820, 300),
    (f: ReturnType<typeof fixture>) => f.move(810, 300, { pointerType: "touch" }),
    (f: ReturnType<typeof fixture>) => f.move(810, 300, { altKey: true }),
    (f: ReturnType<typeof fixture>) => f.move(810, 300, { buttons: 4 }),
  ]) {
    const f = fixture();
    f.move(810, 300);
    stopMove(f);
    assert.equal(f.frames.size, 0);
  }
});

test("blur, hidden/inactive viewports and overlays can stop a stationary hover", () => {
  const f = fixture();
  f.move(810, 620);
  f.disable();
  f.tick();
  assert.equal(f.target.scrollLeft, 300);
  assert.equal(f.target.scrollTop, 200);
  assert.equal(f.frames.size, 0);
});

test("scrolling clamps at the end, stops when no axis can move and resumes in reverse", () => {
  const f = fixture();
  f.target.scrollLeft = 799;
  f.target.scrollTop = 599;
  f.move(810, 620);
  f.tick(); f.tick(16);
  assert.equal(f.target.scrollLeft, 800);
  assert.equal(f.target.scrollTop, 600);
  assert.equal(f.frames.size, 0);
  f.move(12, 22);
  f.tick(32);
  assert.ok(f.target.scrollLeft < 800 && f.target.scrollTop < 600);
});

test("fit-size surfaces do not schedule scrolling; only overflowing axes move", () => {
  const f = fixture();
  f.target.scrollWidth = 800;
  f.target.scrollLeft = 0;
  f.move(810, 620);
  f.tick();
  assert.equal(f.target.scrollLeft, 0);
  assert.ok(f.target.scrollTop > 200);
  f.target.scrollHeight = 600;
  f.target.scrollTop = 0;
  f.tick(16);
  assert.equal(f.frames.size, 0);
});
