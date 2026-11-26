import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import test from "node:test";

const sourceRoot = new URL("../../../src/", import.meta.url);

function sourceFile(path: string): URL {
  return new URL(path, sourceRoot);
}

test("frontend large session and app entrypoints are split into focused helper modules", () => {
  const expectedFiles = [
    "app/composables/vncAppState/localDiagnostics.ts",
    "app/composables/vncAppState/tray.ts",
    "app/composables/vncAppState/viewport.ts",
    "features/sessions/composables/sessionConnectionLifecycle/coreSync.ts",
    "features/sessions/composables/sessionConnectionLifecycle/sessionDisplayEvents.ts",
    "session/protocol/protocolAdapter/nativeAgentRuntime.ts",
    "session/protocol/protocolAdapter/types.ts",
    "session/renderer/coreSurfaceCanvas.ts",
    "session/renderer/coreSurfaceCursor.ts",
    "session/renderer/coreSurfaceSession.ts",
    "features/sessions/components/sessionToolbox/menuOptions.ts",
    "session/transfers/sessionFileUploads.ts",
    "features/sessions/components/sessionToolbox/fileTransfer.ts",
    "features/sessions/components/sessionExternalWindowPage/feedback.ts",
    "features/sessions/components/sessionExternalWindowPage/inputForwarding.ts",
    "features/sessions/components/sessionViewport/dragDrop.ts",
    "features/sessions/components/sessionViewport/hudState.ts",
    "features/sessions/components/sessionViewportGrid/inputForwarding.ts",
    "session/diagnostics/sessionDiagnosticsPanel/appServer.ts",
    "session/diagnostics/sessionDiagnosticsPanel/clipboard.ts",
    "session/diagnostics/sessionDiagnosticsPanel/transport.ts",
  ];

  for (const file of expectedFiles) {
    assert.equal(existsSync(sourceFile(file)), true, `${file} should exist`);
  }

});
