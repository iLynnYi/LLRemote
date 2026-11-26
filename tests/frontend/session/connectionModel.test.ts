import assert from "node:assert/strict";
import test from "node:test";

import {
  connectionModelFromDiscovery,
  connectionModelFromAuthenticatedAgent,
  createInitialConnectionModel,
  formatConnectionModelDiagnostics,
  sessionSupportsCapability,
} from "../../../src/session/connection-model/connectionModel.ts";

test("connection diagnostics expose negotiated identity, transport, auth, and capabilities without secrets", () => {
  const model = connectionModelFromAuthenticatedAgent(
    connectionModelFromDiscovery(
      createInitialConnectionModel({ state: "matched", viewOnly: false }),
      {
        server: { id: "server-1", vendor: "LLRemote", product: "embedded-server", buildVersion: "1.2.3", identityVerification: "server-discovery" },
        agent: { hostMode: "rfb-peer", port: 6200, protocolVersion: "1.0", authenticationMethods: ["local-runtime"] },
      },
    ),
    { serverId: "server-1", viewOnly: false, capabilities: ["file.transfer.upload"] },
  );

  const line = formatConnectionModelDiagnostics(model, "connected");
  assert.match(line, /server_identity=managed/);
  assert.match(line, /discovery=negotiated\(v1\.0\)/);
  assert.match(line, /enhanced_transport=rfb-peer:6200/);
  assert.match(line, /agent_authentication=verified/);
  assert.match(line, /file\.transfer\.upload:supported/);
  assert.doesNotMatch(line, /token|password|secret|bootstrapKey/i);
});

test("initial connection model exposes only standard RFB capabilities", () => {
  const model = createInitialConnectionModel();

  assert.equal(model.baseTransport.kind, "vnc");
  assert.equal(model.serverIdentity.kind, "unknown");
  assert.equal(model.enhancedChannel.status, "unavailable");
  assert.equal(sessionSupportsCapability(model.capabilities, "clipboard.text.best-effort"), false);
  assert.equal(sessionSupportsCapability(model.capabilities, "file.transfer.upload"), false);
});

test("local-runtime describe alone upgrades the Agent channel and effective capabilities", () => {
  const discovered = connectionModelFromDiscovery(
    createInitialConnectionModel({ state: "matched", viewOnly: false }),
    {
        server: { id: "server-1", identityVerification: "server-discovery" },
      policy: { viewOnly: false },
      agent: {
        hostMode: "rfb-peer",
        port: 6200,
        protocolVersion: "1.0",
        authenticationMethods: ["local-runtime"],
      },
    },
  );

  assert.equal(discovered.enhancedChannel.status, "negotiating");
  assert.equal(sessionSupportsCapability(discovered.capabilities, "file.transfer.upload"), false);

  const verified = connectionModelFromAuthenticatedAgent(discovered, {
    serverId: "server-1",
    viewOnly: false,
    capabilities: ["file.transfer.upload"],
  });

  assert.equal(verified.serverIdentity.verification, "authenticated-agent");
  assert.equal(verified.enhancedChannel.status, "available");
  assert.equal(sessionSupportsCapability(verified.capabilities, "file.transfer.upload"), true);
});

test("a mismatched local-runtime description degrades only the Agent channel", () => {
  const model = connectionModelFromDiscovery(
    createInitialConnectionModel({ state: "matched", viewOnly: true }),
    {
      server: { id: "server-1", identityVerification: "server-discovery" },
      policy: { viewOnly: true },
      agent: { hostMode: "rfb-peer", port: 6200, protocolVersion: "1.0", authenticationMethods: ["local-runtime"] },
    },
  );
  const result = connectionModelFromAuthenticatedAgent(model, {
    serverId: "other-server",
    viewOnly: true,
    capabilities: ["file.transfer.upload"],
  });

  assert.equal(result.enhancedChannel.status, "degraded");
  assert.equal(result.enhancedChannel.lastErrorCode, "agent_identity_mismatch");
  assert.equal(sessionSupportsCapability(result.capabilities, "file.transfer.upload"), false);
});

test("a local endpoint match verifies ownership without enabling Agent capabilities", () => {
  const model = createInitialConnectionModel({
    state: "matched",
    viewOnly: true,
    framebufferWidth: 1280,
    framebufferHeight: 800,
  });

  assert.equal(model.serverIdentity.verification, "local-runtime");
  assert.equal(model.enhancedChannel.status, "unavailable");
  assert.equal(model.capabilities.items.find((item) => item.id === "input.pointer")?.state, "supported");
});

test("wire identity claims remain declarations even when they claim local or authenticated verification", () => {
  for (const identityVerification of ["server-discovery", "local-runtime", "authenticated-agent"] as const) {
    const key = crypto.randomUUID();
    const model = connectionModelFromDiscovery(createInitialConnectionModel(), {
      server: { id: "declared-server", vendor: "claimed-vendor", identityVerification },
      agent: { hostMode: "rfb-peer", port: 6200, protocolVersion: "1.0", authenticationMethods: ["agent-ticket"], bootstrapKey: key },
    });
    assert.equal(model.serverIdentity.kind, "unknown");
    assert.equal(model.serverIdentity.verification, "none");
    assert.equal(model.serverIdentity.id, undefined);
    assert.equal(model.serverDeclaration?.id, "declared-server");
    assert.equal(model.serverDeclaration?.vendor, "claimed-vendor");
    assert.equal(JSON.stringify(model).includes(key), false);
    assert.equal(formatConnectionModelDiagnostics(model, "discovery").includes(key), false);
  }
});

test("rediscovery revokes remote Agent identity and capabilities until authentication completes again", () => {
  const manifest = { server: { id: "remote", identityVerification: "server-discovery" as const } };
  const verified = connectionModelFromAuthenticatedAgent(connectionModelFromDiscovery(createInitialConnectionModel(), manifest), {
    serverId: "remote", viewOnly: false, capabilities: ["file.open"],
  });
  assert.equal(verified.serverIdentity.kind, "managed");
  const next = connectionModelFromDiscovery(verified, { server: { id: "replacement" } });
  assert.deepEqual(next.serverIdentity, { kind: "unknown", verification: "none" });
  assert.equal(next.serverDeclaration?.id, "replacement");
  assert.equal(sessionSupportsCapability(next.capabilities, "file.open"), false);
});

test("untrusted Discovery metadata cannot overwrite local-runtime identity evidence", () => {
  const model = connectionModelFromDiscovery(createInitialConnectionModel({ state: "matched", viewOnly: false }), {
    server: { id: "claimed", vendor: "forged", identityVerification: "authenticated-agent" },
  });
  assert.equal(model.serverIdentity.verification, "local-runtime");
  assert.equal(model.serverIdentity.vendor, "LLRemote");
  assert.equal(model.serverDeclaration?.vendor, "forged");
});

test("a failed replacement description clears remote authentication evidence and Agent capabilities", () => {
  const discovered = connectionModelFromDiscovery(createInitialConnectionModel(), { server: { id: "remote", identityVerification: "server-discovery" } });
  const verified = connectionModelFromAuthenticatedAgent(discovered, { serverId: "remote", viewOnly: false, capabilities: ["file.open"] });
  assert.equal(verified.enhancedChannel.status, "available");
  const failed = connectionModelFromAuthenticatedAgent(verified, { serverId: "different", viewOnly: false, capabilities: ["file.reveal"] });
  assert.equal(failed.serverIdentity.kind, "unknown");
  assert.equal(failed.enhancedChannel.authentication, "failed");
  assert.equal(sessionSupportsCapability(failed.capabilities, "file.open"), false);
  assert.equal(failed.baseTransport.kind, "vnc");
});

test("malformed or unbounded Agent descriptions cannot supply identity or effective capabilities", () => {
  const current = connectionModelFromDiscovery(createInitialConnectionModel(), { server: { id: "remote" } });
  for (const description of [null, { serverId: "remote", viewOnly: "false", capabilities: [] },
    { serverId: "remote", viewOnly: false, capabilities: "file.open" },
    { serverId: "remote", viewOnly: false, capabilities: Array(257).fill("file.open") }]) {
    const model = connectionModelFromAuthenticatedAgent(current, description);
    assert.equal(model.serverIdentity.kind, "unknown");
    assert.equal(model.enhancedChannel.status, "degraded");
    assert.equal(model.enhancedChannel.lastErrorCode, "agent_description_invalid");
    assert.equal(sessionSupportsCapability(model.capabilities, "file.open"), false);
  }
});
