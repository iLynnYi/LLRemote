import assert from "node:assert/strict";
import test from "node:test";
import { expandFileTransferSourcesInPages } from "../../../src/session/transfers/fileTransferSources.ts";

test("source scan collects bounded pages and releases a failed scan", async () => {
  let pages = 0, canceled = false;
  const invoke = async (command: string): Promise<unknown> => {
    if (command === "start_file_transfer_source_scan") return "scan-1";
    if (command === "cancel_file_transfer_source_scan") { canceled = true; return; }
    pages++;
    if (pages === 2) throw new Error("unreadable directory");
    return { sources: [{ filePath: "/folder/one", relativePath: "folder/one" }], done: false };
  };
  await assert.rejects(expandFileTransferSourcesInPages(invoke, ["/folder"]), /unreadable directory/);
  assert.equal(canceled, true);
});

test("source scan preserves file order across pages", async () => {
  let page = 0;
  const invoke = async (command: string): Promise<unknown> => {
    if (command === "start_file_transfer_source_scan") return "scan-2";
    if (command === "next_file_transfer_source_page") {
      page++;
      return { sources: [{ filePath: `/folder/${page}` }], done: page === 3 };
    }
    throw new Error("completed scans must not be canceled");
  };
  assert.deepEqual((await expandFileTransferSourcesInPages(invoke, ["/folder"])).map(source => source.filePath),
    ["/folder/1", "/folder/2", "/folder/3"]);
});
