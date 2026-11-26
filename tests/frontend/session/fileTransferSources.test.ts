import assert from "node:assert/strict";
import test from "node:test";
import { selectFileTransferSources } from "../../../src/session/transfers/fileTransferSources.ts";

test("picker cancellation is empty and native failures never expose exception details", async () => {
  assert.deepEqual(await selectFileTransferSources(async () => [], (key) => key), []);
  const paths = ["C:\\用户\\报告.txt", "D:\\资料"];
  assert.deepEqual(await selectFileTransferSources(async () => paths, (key) => key), paths);
  for (const failure of ["native internal detail", { category: "sourcePicker", code: "unavailable" }]) {
    await assert.rejects(selectFileTransferSources(async () => { throw failure; }, (key) => key), { message: "sessions.fileTransfers.sourcePickerUnavailable" });
  }
  await assert.rejects(selectFileTransferSources(async () => { throw { category: "sourcePicker", code: "invalidPath" }; }, (key) => key), { message: "sessions.fileTransfers.sourcePathInvalid" });
});
