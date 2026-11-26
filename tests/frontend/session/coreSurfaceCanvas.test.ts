import assert from "node:assert/strict";
import test from "node:test";
import { decodeCoreSurfaceFrame } from "../../../src/session/renderer/coreSurfaceCanvas.ts";

test("core surface packet exposes a zero-copy Canvas RGBA rectangle view", () => {
  const packet = new Uint8Array(48);
  packet.set([86, 78, 67, 70, 1, 0, 0, 28]);
  new DataView(packet.buffer).setBigUint64(8, 3n);
  new DataView(packet.buffer).setUint32(16, 2);
  new DataView(packet.buffer).setUint32(20, 1);
  new DataView(packet.buffer).setUint16(24, 1);
  new DataView(packet.buffer).setUint32(28, 1);
  new DataView(packet.buffer).setUint32(36, 1);
  new DataView(packet.buffer).setUint32(40, 1);
  packet.set([30, 20, 10, 255], 44);
  const decoded = decodeCoreSurfaceFrame(packet.buffer);
  assert.equal(decoded.frameId, 3n);
  assert.deepEqual([...decoded.rectangles[0].pixels], [30, 20, 10, 255]);
  decoded.rectangles[0].pixels[0] = 9;
  assert.equal(packet[44], 9, "renderer must view packet bytes without a second pixel copy");
});
