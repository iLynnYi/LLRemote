import { listen } from "@tauri-apps/api/event";
import { parseServerDiscoveryManifest } from "../discovery/serverDiscovery.ts";
import { formatConnectionModelDiagnostics, sessionSupportsCapability } from "../connection-model/connectionModel.ts";
import { createCoreSurfaceSession } from "../renderer/coreSurfaceSession.ts";
import type { CoreSurfaceSession } from "../renderer/coreSurfaceSession.ts";
import { applyCoreSurfaceCursor, type CoreSurfaceCursor } from "../renderer/coreSurfaceCursor.ts";
import { createSessionDisplayInfo } from "../viewport/sessionDisplay.ts";
import { qualityProfileSettings } from "../settings/sessionQuality.ts";
import { createViewportRuntimeEvidence, type ViewportRuntimeEvidence } from "../diagnostics/runtimeEvidence.ts";
import { createNativeAgentRuntime } from "./protocolAdapter/nativeAgentRuntime.ts";
import { remoteMessageTextError } from "../messages/remoteMessage.ts";
import { createServerRuntime } from "./protocolAdapter/serverRuntime.ts";
import { createConnectionIssue } from "../diagnostics/sessionDiagnostics.ts";
import { nativeRfbError, NativeRfbError, readNativeRfbFailure, type NativeRfbFailure } from "./rfbFailure.ts";
import type {
  CreateVncProtocolAdapterOptions,
  ProtocolAdapter,
  ProtocolConnection,
  RemoteSessionRfbEvent,
} from "./protocolAdapter/types.ts";

export type * from "./protocolAdapter/types.ts";

const STARTUP_DISPLAY_WAKE_DELAY_MS = 1500;

/** Native-only VNC adapter. TCP, RFB framing and decode stay in Rust Core. */
export function createVncProtocolAdapter(options: CreateVncProtocolAdapterOptions): ProtocolAdapter {
  const disconnects = new WeakMap<ProtocolConnection, Promise<void>>();

  return {
    protocol: "vnc",

    async connect(connectOptions) {
      let connectionPassword = connectOptions.password;
      // Long-lived connection actions retain callbacks/configuration, not the
      // transient password handed to the native authentication command.
      connectOptions = { ...connectOptions, password: undefined };
      const sessionId = connectOptions.sessionId;
      const surfaceUrl = connectOptions.surfaceUrl;
      if (!sessionId || !surfaceUrl) throw new Error("native_surface_context_missing");

      let serverViewOnly = Boolean(connectOptions.connectionModel?.localEndpointMatch.state === "matched"
        && connectOptions.connectionModel.localEndpointMatch.viewOnly);
      let readonly = Boolean(connectOptions.readonly) || serverViewOnly;
      let inputEnabled = false;
      let transportReady = false;
      let connected = false;
      let handedOff = false;
      let closed = false;
      let disconnecting: Promise<void> | null = null;
      let startupDisplayWakeTimer: ReturnType<typeof setTimeout> | null = null;
      let startupDisplayWakeAttempted = false;
      let clipboardMode = connectOptions.clipboardSyncMode ?? "off";
      let displayInfo = null as ReturnType<typeof createSessionDisplayInfo> | null;
      let attemptFailure: { error: Error; report: () => void } | null = null;
      let unlisten: (() => void) | null = null;
      let stopAbort: (() => void) | null = null;
      let surface: CoreSurfaceSession | null = null;
      let pendingCursor: CoreSurfaceCursor | null = null;
      let runtimeEvidence: ViewportRuntimeEvidence | null = null;
      let presentedFrames = 0;
      let firstFrameAtMs = 0;
      let latestFrameAtMs = 0;
      let totalFrameIntervalMs = 0;
      let maxFrameIntervalMs = 0;
      let surfaceWindowStarted = performance.now();
      let surfaceWindowFrames = 0;
      let surfaceLastCommitAt: number | null = null;
      let surfaceWindowMaxInterval = 0;
      const heldKeys = new Set<number>();
      let heldPointer: { x: number; y: number; buttonMask: number } | null = null;

      const agent = createNativeAgentRuntime({
        invokeCommand: options.invoke,
        host: connectOptions.host,
        vncPort: connectOptions.port,
        initialConnectionModel: connectOptions.connectionModel,
        onConnectionModelChanged: connectOptions.onConnectionModelChanged,
        onRemoteClipboardText: (text) => {
          if (clipboardMode !== "off") void connectOptions.onRemoteClipboardText?.(text, "agent");
        },
        onEvent: connectOptions.onEvent,
      });

      const hasAgentDisplayWake = () => {
        const { enhancedChannel, capabilities } = agent.connectionModel;
        return enhancedChannel.status === "available" && sessionSupportsCapability(capabilities, "display.wake");
      };
      const requestAgentDisplayWake = async () => {
        const response = await agent.invoke("display.wake");
        if (response.ok !== true) {
          throw new Error(typeof response.error === "string" ? response.error : "display_wake_failed");
        }
      };
      const canAttemptStartupDisplayWake = () => !transportReady && !closed && !attemptFailure
        && !readonly && !startupDisplayWakeAttempted && hasAgentDisplayWake();
      const cancelStartupDisplayWake = () => {
        if (startupDisplayWakeTimer !== null) clearTimeout(startupDisplayWakeTimer);
        startupDisplayWakeTimer = null;
      };
      const scheduleStartupDisplayWake = () => {
        if (startupDisplayWakeTimer !== null || !canAttemptStartupDisplayWake()) return;
        // Agent Discovery can finish before native startup commits its first frame.
        startupDisplayWakeTimer = setTimeout(() => {
          startupDisplayWakeTimer = null;
          if (!canAttemptStartupDisplayWake()) return;
          startupDisplayWakeAttempted = true;
          void requestAgentDisplayWake().catch(() => {
            if (!transportReady && !closed && !attemptFailure) {
              connectOptions.onEvent("display-wake", "Failed to wake the remote display while waiting for the first frame.", "warning");
            }
          });
        }, STARTUP_DISPLAY_WAKE_DELAY_MS);
      };

      const reportNativeFailure = (failure: NativeRfbFailure) => {
        if (connectOptions.onConnectionIssue) {
          connectOptions.onConnectionIssue(createConnectionIssue({
            reason: failure, phase: connected ? "disconnected" : "tcpConnecting",
          }));
        } else {
          connectOptions.onError(failure.code);
        }
      };

      const runtime = createServerRuntime({
        sendRequest: (requestId) => sendTransportCommand({ type: "secureAttention", requestId }),
        refresh: () => { void sendTransportCommand({ type: "refresh", incremental: false }); },
        onChanged: () => connectOptions.onServerRuntimeChanged?.(),
      });
      const disposeLocal = () => {
        cancelStartupDisplayWake();
        runtime.dispose();
        heldKeys.clear();
        heldPointer = null;
        stopAbort?.();
        stopAbort = null;
        unlisten?.();
        unlisten = null;
        agent.dispose();
        surface?.disconnect();
      };
      // A terminal event can overtake the startup command or a queued Canvas
      // callback. Preserve the first failure so neither can revive this attempt.
      const throwIfFailed = () => {
        if (attemptFailure) throw attemptFailure.error;
      };
      const failAttempt = (error: Error, report: () => void) => {
        if (closed || attemptFailure) return;
        attemptFailure = { error, report };
        transportReady = false;
        inputEnabled = false;
        disposeLocal();
        // Before connect returns, its caller owns failure reporting via reject.
        if (handedOff) attemptFailure.report();
      };
      const failTransport = (failure: NativeRfbFailure) => {
        failAttempt(new NativeRfbError(failure), () => {
          reportNativeFailure(failure);
          connectOptions.onStatus("disconnected unexpectedly");
        });
      };

      const signal = connectOptions.signal;
      const abort = () => {
        connectionPassword = undefined;
        failAttempt(new Error("session_connect_cancelled"), () => undefined);
      };
      if (signal) {
        signal.addEventListener("abort", abort, { once: true });
        stopAbort = () => signal.removeEventListener("abort", abort);
        if (signal.aborted) abort();
      }
      throwIfFailed();

      const sendTransportCommand = (input: Record<string, unknown>, propagateError = false): Promise<void> => {
        if (!transportReady || closed) return Promise.resolve();
        return options.invoke("send_remote_session_rfb_input", {
          request: { sessionId, input },
        }).then(() => {}, (error) => {
          const failure = nativeRfbError(error);
          failTransport(failure.failure);
          if (propagateError) throw failure;
        });
      };
      const sendInput = (input: Record<string, unknown>) => {
        if (readonly || !inputEnabled) return Promise.resolve();
        if (runtime.state && runtime.state.desktop !== "available") return Promise.resolve();
        if (input.type === "key" && typeof input.keysym === "number") {
          if (input.down) {
            if (!heldKeys.has(input.keysym) && heldKeys.size >= 256) return Promise.resolve();
            heldKeys.add(input.keysym);
          } else heldKeys.delete(input.keysym);
        } else if (input.type === "pointer" && typeof input.x === "number" && typeof input.y === "number" && typeof input.buttonMask === "number") {
          heldPointer = { x: input.x, y: input.y, buttonMask: input.buttonMask & 7 };
        }
        return sendTransportCommand(input);
      };

      const releaseHeldInput = () => {
        for (const keysym of heldKeys) void sendTransportCommand({ type: "key", down: false, keysym });
        heldKeys.clear();
        if (heldPointer?.buttonMask) void sendTransportCommand({ type: "pointer", x: heldPointer.x, y: heldPointer.y, buttonMask: 0 });
        heldPointer = null;
      };

      const handleRfbEvent = (event: RemoteSessionRfbEvent) => {
        if (closed || attemptFailure) return;
        switch (event.type) {
          case "serverRuntime":
            runtime.consume(event.notice);
            return;
          case "performance": {
            const m = event.metrics;
            connectOptions.onEvent("rfb-performance",
              `rfb_metrics window_ms=${m.windowMs} received_bytes=${m.receivedBytes} updates=${m.updates} committed_frames=${m.committedFrames} decode_total_us=${m.decodeTotalUs} decode_max_us=${m.decodeMaxUs} read_calls=${m.readCalls} read_timeouts=${m.readTimeouts} read_total_us=${m.readTotalUs} consume_total_us=${m.consumeTotalUs} write_total_us=${m.writeTotalUs} write_max_us=${m.writeMaxUs} event_total_us=${m.eventTotalUs} rectangles=${m.encodings.rectangles} copy_rect=${m.encodings.copyRect} tight=${m.encodings.tight} jpeg=${m.encodings.jpeg}`,
              "info");
            return;
          }
          case "bell":
            connectOptions.onEvent("rfb", "Remote bell received.", "info");
            return;
          case "clipboard":
            if (clipboardMode !== "off") {
              void connectOptions.onRemoteClipboardText?.(event.text, "rfb");
              connectOptions.onClipboardActivity?.({
                direction: "server-to-client",
                atMs: Date.now(),
                accepted: true,
                reason: "remote-received",
                mode: clipboardMode,
                bytes: new TextEncoder().encode(event.text).length,
                channel: "rfb",
              });
            }
            return;
          case "cursor":
            pendingCursor = event.cursor;
            if (surface) applyCoreSurfaceCursor(surface.canvas, event.cursor);
            return;
          case "pointerPosition":
            return;
          case "desktopLayout": {
            if (!event.screens.length) return;
            const left = Math.min(...event.screens.map((screen) => screen.x));
            const top = Math.min(...event.screens.map((screen) => screen.y));
            const right = Math.max(...event.screens.map((screen) => screen.x + screen.width));
            const bottom = Math.max(...event.screens.map((screen) => screen.y + screen.height));
            displayInfo = {
              ...createSessionDisplayInfo(right - left, bottom - top),
              multiDisplayHint: event.screens.length > 1,
            };
            connectOptions.onDisplayInfo?.(displayInfo);
            return;
          }
          case "discoveryManifest": {
            // A restriction declared by the connected RFB peer does not depend
            // on local endpoint matching or optional Agent authentication.
            const manifest = parseServerDiscoveryManifest(event.manifest);
            if (!serverViewOnly && manifest?.policy?.viewOnly === true) {
              serverViewOnly = true;
              readonly = true;
              connectOptions.onViewOnlySharedChanged?.(true);
            }
            void agent.consumeDiscovery(event.manifest).then(() => {
              if (closed || attemptFailure) return;
              if (
                !serverViewOnly && agent.connectionModel.localEndpointMatch.state === "matched"
                && agent.connectionModel.localEndpointMatch.viewOnly
              ) {
                serverViewOnly = true;
                readonly = true;
                connectOptions.onViewOnlySharedChanged?.(true);
              }
              scheduleStartupDisplayWake();
            });
            return;
          }
          case "discoveryError":
            agent.invalidateDiscovery("server_discovery_error");
            connectOptions.onEvent("discovery", "Server Discovery error; code=server_discovery_error.", "warning");
            return;
          case "discoveryFailure":
            agent.invalidateDiscovery("server_discovery_failed");
            connectOptions.onEvent("discovery", "Server Discovery unavailable; code=server_discovery_failed.", "warning");
            return;
          case "disconnected":
            failTransport(nativeRfbError(event.failure).failure);
        }
      };

      try {
        unlisten = options.listenRfbEvent
          ? await options.listenRfbEvent(`remote-session-rfb-event:${sessionId}`, handleRfbEvent)
          : await listen<RemoteSessionRfbEvent>(
            `remote-session-rfb-event:${sessionId}`,
            (event) => handleRfbEvent(event.payload),
          );

        throwIfFailed();
        connectOptions.onStatus("connecting");
        let promptedForCredentials = false;
        while (true) {
          throwIfFailed();
          try {
            await options.invoke("connect_remote_session_rfb", {
              request: {
                sessionId,
                host: connectOptions.host,
                port: connectOptions.port,
                password: connectionPassword,
              },
            });
            throwIfFailed();
            break;
          } catch (error) {
            throwIfFailed();
            if (
              readNativeRfbFailure(error)?.category !== "credentials_required"
              || Boolean(connectionPassword)
              || promptedForCredentials
              || !connectOptions.onCredentialsRequired
            ) {
              throw nativeRfbError(error);
            }
            promptedForCredentials = true;
            connectionPassword = await connectOptions.onCredentialsRequired();
            if (!connectionPassword) {
              throw new NativeRfbError({ category: "authentication_cancelled", code: "rfb_authentication_cancelled", retryable: true });
            }
          }
        }
      } catch (error) {
        disposeLocal();
        throwIfFailed();
        throw error;
      } finally {
        cancelStartupDisplayWake();
        connectionPassword = undefined;
      }
      transportReady = true;

      try {
        surface = (options.createSurfaceSession ?? createCoreSurfaceSession)({
          target: connectOptions.target,
          surfaceUrl,
          onFrameCommitted: (commit) => {
            if (closed || attemptFailure) return;
            if (runtime.framePresented()) releaseHeldInput();
            const monotonicNow = performance.now();
            const interval = surfaceLastCommitAt === null ? 0 : monotonicNow - surfaceLastCommitAt;
            surfaceLastCommitAt = monotonicNow;
            surfaceWindowFrames += 1;
            surfaceWindowMaxInterval = Math.max(surfaceWindowMaxInterval, interval);
            const windowMs = monotonicNow - surfaceWindowStarted;
            if (windowMs >= 5000) {
              connectOptions.onEvent("surface-performance",
                `surface_metrics window_ms=${Math.round(windowMs)} presented_frames=${surfaceWindowFrames} fps=${(surfaceWindowFrames * 1000 / windowMs).toFixed(2)} latest_interval_ms=${interval.toFixed(2)} max_interval_ms=${surfaceWindowMaxInterval.toFixed(2)} framebuffer=${commit.width}x${commit.height}`,
                "info");
              surfaceWindowStarted = monotonicNow;
              surfaceWindowFrames = 0;
              surfaceWindowMaxInterval = 0;
            }
            const now = Date.now();
            const frameIntervalMs = latestFrameAtMs > 0 ? now - latestFrameAtMs : undefined;
            if (!firstFrameAtMs) firstFrameAtMs = now;
            if (frameIntervalMs !== undefined) {
              totalFrameIntervalMs += frameIntervalMs;
              maxFrameIntervalMs = Math.max(maxFrameIntervalMs, frameIntervalMs);
            }
            latestFrameAtMs = now;
            presentedFrames += 1;
            displayInfo = createSessionDisplayInfo(commit.width, commit.height);
            runtimeEvidence = createViewportRuntimeEvidence({
              canvasSizes: [{ width: surface?.canvas.width ?? commit.width, height: surface?.canvas.height ?? commit.height }],
              framebuffer: { width: commit.width, height: commit.height },
              transportId: `native:${sessionId}`,
              latestCoreEvent: `surface_frame:${commit.frameId}`,
              latestCoreEventAtMs: now,
              displaySource: "core_framebuffer",
              inputPath: "native_rfb_input",
              surface: {
                presentedFrames,
                firstFrameAtMs,
                latestFrameAtMs,
                estimatedFps: now > firstFrameAtMs ? ((presentedFrames - 1) * 1000) / (now - firstFrameAtMs) : undefined,
                latestFrameIntervalMs: frameIntervalMs,
                maxFrameIntervalMs: maxFrameIntervalMs || undefined,
                averageFrameIntervalMs: presentedFrames > 1 ? totalFrameIntervalMs / (presentedFrames - 1) : undefined,
                framebuffer: { width: commit.width, height: commit.height },
              },
            });
            connectOptions.onDisplayInfo?.(displayInfo);
            connectOptions.onRuntimeEvidence?.(runtimeEvidence);
            if (!connected) {
              connected = true;
              connectOptions.onEvent(
                "connection-model",
                formatConnectionModelDiagnostics(agent.connectionModel, "connected"),
                "info",
              );
              connectOptions.onStatus("connected");
            }
          },
          onError: (code) => failAttempt(new Error(code), () => connectOptions.onError(code)),
        });
        throwIfFailed();
      } catch (error) {
        const surfaceError = error instanceof Error ? error : new Error("surface_startup_failed");
        failAttempt(surfaceError, () => connectOptions.onError(surfaceError.message));
        disposeLocal();
        throwIfFailed();
        throw surfaceError;
      }
      if (pendingCursor) applyCoreSurfaceCursor(surface.canvas, pendingCursor);

      const connection: ProtocolConnection = {
        protocol: "vnc",
        get connectionModel() { return agent.connectionModel; },
        baseTransport: agent.connectionModel.baseTransport,
        displaySource: "vnc",
        inputSource: "vnc",
        getServerRuntimeState: () => runtime.state,
        notifySurfacePresented: (recoveryId) => {
          if (runtime.framePresented(recoveryId)) releaseHeldInput();
        },
        sendSecureAttention: async () => {
          if (readonly || !inputEnabled || closed) return "denied";
          if (runtime.state) return runtime.request();
          // Send the standard key sequence when the server has no result channel.
          for (const [down, keysym] of [[true, 0xffe3], [true, 0xffe9], [true, 0xffff],
            [false, 0xffff], [false, 0xffe9], [false, 0xffe3]] as const) {
            await sendInput({ type: "key", down, keysym });
            if (closed || attemptFailure) return "failed";
          }
          return "unconfirmed";
        },
        transport: { id: `native:${sessionId}`, surfaceUrl },
        disconnect: () => {
          if (disconnecting) return disconnecting;
          if (!closed) {
            closed = true;
            transportReady = false;
            disposeLocal();
          }
          const task = Promise.resolve().then(async () => {
            try {
              await options.invoke("disconnect_remote_session_rfb", { sessionId });
            } catch (error) {
              throw nativeRfbError(error);
            }
          });
          disconnecting = task;
          void task.catch(() => { if (disconnecting === task) disconnecting = null; });
          return task;
        },
        focus: () => surface?.canvas.focus(),
        refreshDisplay: () => sendTransportCommand({ type: "refresh", incremental: false }),
        setInputEnabled: (enabled) => {
          if (inputEnabled && !enabled) releaseHeldInput();
          inputEnabled = enabled;
        },
        setReadonlyMode: (enabled) => { readonly = Boolean(serverViewOnly) || enabled; },
        setClipboardSyncMode: (mode) => { clipboardMode = mode; },
        forwardExternalInput: (payload) => {
          if (payload.type === "keyboard") sendInput({ type: "key", down: payload.down, keysym: payload.keysym });
          else if (payload.type === "pointer") sendInput({ type: "pointer", x: payload.x, y: payload.y, buttonMask: payload.buttonMask });
          else if (payload.type === "wheel") {
            const wheel = payload.deltaY < 0 ? 0x08 : payload.deltaY > 0 ? 0x10 : payload.deltaX < 0 ? 0x20 : 0x40;
            sendInput({ type: "pointer", x: payload.x, y: payload.y, buttonMask: payload.buttonMask | wheel });
            sendInput({ type: "pointer", x: payload.x, y: payload.y, buttonMask: payload.buttonMask });
          }
        },
        sendRemoteKeyEvent: (payload) => sendInput({ type: "key", down: payload.down, keysym: payload.keysym }),
        releaseRemoteInput: releaseHeldInput,
        sendRemoteKeySequence: async (payloads) => {
          // Await native enqueueing so modifier/down/up order survives IPC scheduling.
          for (const payload of payloads) {
            if (closed || readonly || !inputEnabled) break;
            await sendInput({ type: "key", down: payload.down, keysym: payload.keysym });
          }
        },
        controlRemoteDesktop: async (actionId) => { await agent.invoke("remote.desktop.control", { actionId }); },
        hasAgentChannel: () => !closed && !attemptFailure && agent.connectionModel.enhancedChannel.status === "available",
        sendRemoteMessage: async (text) => {
          const invalid = remoteMessageTextError(text);
          if (invalid) throw new Error(invalid);
          if (closed || attemptFailure || !transportReady || !connected) throw new Error("remote_message_disconnected");
          if (readonly || serverViewOnly) throw new Error("agent_view_only");
          if (runtime.state && runtime.state.desktop !== "available") throw new Error("agent_user_desktop_unavailable");
          if (agent.connectionModel.enhancedChannel.status !== "available"
            || !sessionSupportsCapability(agent.connectionModel.capabilities, "message.show")) {
            throw new Error("remote_message_unsupported");
          }
          const response = await agent.invoke("message.show", { text });
          if (response.ok !== true) {
            throw new Error(typeof response.error === "string" ? response.error : "remote_message_display_failed");
          }
        },
        sendRemoteClipboardText: async (text) => {
          // Client clipboard changes are broadcast even while another app/window
          // owns keyboard focus. Input ownership only gates keys and pointers.
          if (closed || !transportReady || readonly
            || (runtime.state && runtime.state.desktop !== "available")) return "skipped";
          const agentWritable = agent.connectionModel.enhancedChannel.status === "available"
            && sessionSupportsCapability(agent.connectionModel.capabilities, "clipboard.text.write");
          let channel: "agent" | "rfb";
          if (agentWritable) {
            const response = await agent.invoke("clipboard.text.write", { text });
            if (response.ok !== true) throw new Error(typeof response.error === "string" ? response.error : "agent_clipboard_write_failed");
            channel = "agent";
          } else {
            await sendTransportCommand({ type: "clipboard", text }, true);
            channel = "rfb";
          }
          connectOptions.onClipboardActivity?.({
            direction: "client-to-server", atMs: Date.now(), accepted: true,
            reason: channel === "agent" ? "remote-written" : "sent", mode: clipboardMode,
            bytes: new TextEncoder().encode(text).length, channel,
          });
          return channel === "agent" ? "written" : "sent-unconfirmed";
        },
        uploadRemoteFile: agent.upload,
        controlRemoteFileTransfer: async (transferId, action) => { await agent.invoke(`file.transfer.${action}`, { transferId }); },
        revealRemoteFile: async (path) => { await agent.invoke("file.reveal", { path }); },
        openRemoteFile: async (path) => { await agent.invoke("file.open", { path }); },
        diagnoseRemotePermissions: async () => {
          const response = await agent.invoke("agent.permissions.diagnose");
          return { target: `${connectOptions.host}:${connectOptions.port}`, checkedAt: new Date().toISOString(), items: response.permissions ?? [] } as never;
        },
        enumerateRemoteDisplays: async () => {
          const response = await agent.invoke("agent.display.enumerate");
          return { target: `${connectOptions.host}:${connectOptions.port}`, checkedAt: new Date().toISOString(), source: "agent", displays: response.displays ?? [] } as never;
        },
        selectRemoteDisplay: async (displayId) => { await agent.invoke("agent.display.select", { displayId }); },
        recoverSecureInput: async (reason) => await agent.invoke("agent.secure_input.recover", { reason }) as never,
        wakeRemoteDisplay: async () => {
          if (closed || !transportReady || !connected) throw new Error("display_wake_disconnected");
          if (readonly) throw new Error("agent_view_only");
          if (hasAgentDisplayWake()) {
            await requestAgentDisplayWake();
            return "submitted";
          }
          // The compatibility gesture is ordinary RFB input and follows viewport input ownership.
          for (const coordinate of [1, 2]) {
            if (closed || !transportReady) throw new Error("display_wake_disconnected");
            if (readonly || !inputEnabled || heldPointer?.buttonMask) throw new Error("display_wake_input_unavailable");
            if (runtime.state && runtime.state.desktop !== "available") throw new Error("agent_user_desktop_unavailable");
            await sendTransportCommand({ type: "pointer", x: coordinate, y: coordinate, buttonMask: 0 }, true);
          }
          return "sent-unconfirmed";
        },
        notifyRemoteSessionEnd: async () => {
          if (closed || attemptFailure) throw new Error("agent_channel_unavailable");
          if (!sessionSupportsCapability(agent.connectionModel.capabilities, "remote.session.end")) {
            return;
          }
          const response = await agent.invoke("remote.session.end");
          if (response.ok !== true) throw new Error(typeof response.error === "string" ? response.error : "agent_session_end_failed");
        },
        getDisplayInfo: () => displayInfo,
        getRuntimeEvidence: () => runtimeEvidence,
        setQualityProfile: (profile) => {
          const settings = qualityProfileSettings(profile);
          sendTransportCommand({ type: "setQuality", ...settings });
        },
        setViewportScaleMode: () => undefined,
      };
      handedOff = true;
      return connection;
    },

    disconnect(connection) {
      const existing = disconnects.get(connection);
      if (existing) return existing;
      const task = Promise.resolve().then(() => connection.disconnect());
      disconnects.set(connection, task);
      void task.catch(() => { if (disconnects.get(connection) === task) disconnects.delete(connection); });
      return task;
    },
  };
}
