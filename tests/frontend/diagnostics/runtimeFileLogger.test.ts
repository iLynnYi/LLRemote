import assert from "node:assert/strict";
import test from "node:test";
import { summarizePerformanceLog } from "../../../src/diagnostics/runtimeFileLogger.ts";

test("terminal performance summaries keep the useful measurements", () => {
  assert.equal(
    summarizePerformanceLog("surface-performance", "surface_metrics presented_frames=19 fps=3.26 max_interval_ms=1047.00"),
    "画面性能：3.26 帧/秒，19 帧，最长间隔 1047.00 毫秒",
  );
  assert.equal(
    summarizePerformanceLog("rfb-performance", "rfb_metrics received_bytes=5606304 updates=18 committed_frames=18"),
    "连接性能：18 次更新，18 帧，接收 5606304 字节",
  );
  assert.equal(summarizePerformanceLog("会话", "connected"), undefined);
});
