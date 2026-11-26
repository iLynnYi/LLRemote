import assert from "node:assert/strict";
import test from "node:test";
import { droppedFilePaths, isFileTransferDragEvent } from "../../../src/features/sessions/components/sessionViewport/dragDrop.ts";
import { createSessionViewportHudState } from "../../../src/features/sessions/components/sessionViewport/hudState.ts";

test("viewport HUD keeps per-session state independent between windows", () => {
  const gridHud = createSessionViewportHudState();
  const externalHud = createSessionViewportHudState("first");

  assert.equal(gridHud.isViewportHudExpanded("first"), false);
  assert.equal(externalHud.isViewportHudExpanded("first"), true);
  assert.equal(externalHud.isViewportHudExpanded("second"), false);
  assert.equal(externalHud.isViewportHudHidden("first"), false);

  gridHud.toggleViewportHudExpanded("second");
  externalHud.toggleViewportHud("first");
  assert.equal(gridHud.isViewportHudExpanded("second"), true);
  assert.equal(externalHud.isViewportHudHidden("first"), true);
  assert.equal(externalHud.isViewportHudHidden("second"), false);
  assert.equal(gridHud.isViewportHudHidden("first"), false);
  assert.equal(externalHud.isViewportHudExpanded("first"), true);

  externalHud.toggleViewportHud("first");
  externalHud.toggleViewportHudExpanded("first");
  assert.equal(externalHud.isViewportHudHidden("first"), false);
  assert.equal(externalHud.isViewportHudExpanded("first"), false);
});

test("viewport drag and drop combines file paths and local URIs without duplicates", () => {
  const event = {
    dataTransfer: {
      types: ["Files", "text/uri-list"],
      files: [{ path: "/tmp/first.txt" }, { name: "no-path.txt" }],
      getData: () => "# comment\r\nfile:///tmp/first.txt\r\nfile:///tmp/second%20file.txt\r\nhttps://example.test/ignored\r\nfile:///tmp/bad%",
    },
  } as unknown as DragEvent;

  assert.equal(isFileTransferDragEvent(event), true);
  assert.deepEqual(droppedFilePaths(event), ["/tmp/first.txt", "/tmp/second file.txt"]);
  assert.equal(isFileTransferDragEvent({ dataTransfer: null } as DragEvent), false);
  assert.deepEqual(droppedFilePaths({ dataTransfer: null } as DragEvent), []);
});
