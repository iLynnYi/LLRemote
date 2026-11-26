import { ref } from "vue";
import { localVncServerPasswordValidationKey } from "../../../server/localShare.ts";
import type { BackgroundShareOverview, BackgroundShareStartRequest, createBackgroundShareApi } from "@server/backgroundShare";
import type { LocalVncServerPermissionKind } from "@server/localShare";

export type LocalShareStartPhase = "idle" | "preparing" | "updating" | "awaiting_permission" | "awaiting_password" | "requesting_permission" | "recovering" | "starting";
type Options = {
  api: Pick<ReturnType<typeof createBackgroundShareApi>, "prepare" | "permission" | "start" | "restart">;
  readStatus: () => Promise<BackgroundShareOverview>;
  acceptOverview: (overview: BackgroundShareOverview) => void;
  setBusy: (busy: boolean) => void;
  onStarted: (overview: BackgroundShareOverview) => void;
  onPermissionGranted?: () => void;
  onError: (error: unknown) => void;
  now?: () => number;
  scheduleExpiry?: (expire: () => void, delayMs: number) => () => void;
};
type Attempt = {
  request: BackgroundShareStartRequest | null;
  required: LocalVncServerPermissionKind[];
  expiresAt: number;
  prompted: Set<LocalVncServerPermissionKind>;
  recovered: boolean;
  cancelExpiry: (() => void) | null;
  allowTemporaryPassword: boolean;
};

/** Owns one bounded startup intent. No credential enters reactive state,
 * diagnostics or persistence while the user approves system permissions. */
export function useLocalShareStartFlow(options: Options) {
  const phase = ref<LocalShareStartPhase>("idle");
  const purpose = ref<"start" | "permission" | "repair">("start");
  const requiredPermissions = ref<LocalVncServerPermissionKind[]>([]);
  const missingPermissions = ref<LocalVncServerPermissionKind[]>([]);
  const signatureChangedPermissions = ref<LocalVncServerPermissionKind[]>([]);
  const now = options.now ?? Date.now;
  const scheduleExpiry = options.scheduleExpiry ?? ((expire, delayMs) => {
    const timer = globalThis.setTimeout(expire, delayMs);
    return () => globalThis.clearTimeout(timer);
  });
  let attempt: Attempt | null = null;
  let work: Promise<void> | null = null;
  let authorizing = false;
  let returnedToApp = false;
  let disposed = false;

  function current(value: Attempt): boolean {
    return !disposed && attempt === value;
  }

  function finish(value: Attempt) {
    value.cancelExpiry?.();
    value.cancelExpiry = null;
    if (value.request) value.request.password = "";
    if (attempt !== value) return;
    const changingHost = purpose.value === "repair" || phase.value === "requesting_permission";
    attempt = null;
    phase.value = "idle";
    missingPermissions.value = [];
    requiredPermissions.value = [];
    signatureChangedPermissions.value = [];
    // A cancelled authorization or maintenance request may still be changing the host.
    // Keep lifecycle controls locked until that IPC operation has settled.
    if (!disposed) options.setBusy(changingHost && work !== null);
  }

  function valid(value: Attempt): boolean {
    if (!current(value)) return false;
    if (now() < value.expiresAt) return true;
    expire(value);
    return false;
  }

  function expire(value: Attempt) {
    if (!current(value)) return;
    finish(value);
    options.onError(new Error(value.request?.temporaryPassword
      ? "background_share_temporary_password_timeout" : "background_share_permission_timeout"));
  }

  function armExpiry(value: Attempt) {
    value.expiresAt = now() + 120_000;
    // Expiry releases the intent without polling permissions or starting a listener.
    value.cancelExpiry = scheduleExpiry(() => expire(value), 120_000);
  }

  function missing(overview: BackgroundShareOverview, value: Attempt) {
    return value.required.filter(kind => !overview.server.permissions.some(permission =>
      permission.kind === kind && permission.status === "granted"));
  }

  function run(value: Attempt, action: () => Promise<void>): Promise<void> {
    const operation = action().catch(error => {
      if (current(value)) { finish(value); options.onError(error); }
    }).finally(() => {
      if (work === operation) work = null;
      if (!disposed && !attempt) options.setBusy(false);
      if (returnedToApp && current(value) && phase.value === "awaiting_permission") {
        returnedToApp = false;
        void resume(true);
      }
    });
    work = operation;
    return operation;
  }

  async function advance(value: Attempt, overview: BackgroundShareOverview): Promise<void> {
    if (!valid(value)) return;
    options.acceptOverview(overview);
    if (value.request && overview.server.status === "running") {
      finish(value);
      options.onStarted(overview);
      return;
    }
    const required = missing(overview, value);
    missingPermissions.value = required;
    signatureChangedPermissions.value = (overview.signatureChangedPermissions ?? []).filter(kind => required.includes(kind));
    if (!required.length) {
      if (!value.request) {
        finish(value);
        options.onPermissionGranted?.();
        return;
      }
      phase.value = "starting";
      value.cancelExpiry?.();
      value.cancelExpiry = null;
      options.setBusy(true);
      try {
        const started = await startRequest(value.request);
        if (current(value)) { finish(value); options.onStarted(started); }
      } catch (error) {
        const code = error instanceof Error ? error.message : typeof error === "string" ? error : "";
        if (!current(value) || !value.allowTemporaryPassword || value.request.temporaryPassword
          || !/^background_share_(?:credential_unavailable|initial_credential_rollback_failed)(?::|$)/.test(code)) throw error;
        value.request.temporaryPassword = true;
        armExpiry(value);
        if (value.request.password) {
          await retryTemporaryStart(value);
        } else {
          phase.value = "awaiting_password";
          options.setBusy(false);
        }
      }
      return;
    }
    phase.value = "awaiting_permission";
    options.setBusy(false);
  }

  async function startRequest(request: BackgroundShareStartRequest) {
    const outgoing = { ...request };
    try { return await options.api.start(outgoing); }
    finally { outgoing.password = ""; }
  }

  async function retryTemporaryStart(value: Attempt): Promise<void> {
    options.setBusy(true);
    phase.value = "preparing";
    const overview = await options.readStatus();
    if (!valid(value)) return;
    await advance(value, overview);
  }

  async function begin(request: BackgroundShareStartRequest): Promise<void> {
    if (disposed || attempt || work) {
      request.password = "";
      if (!disposed && !attempt && !authorizing && purpose.value !== "repair") options.setBusy(false);
      return;
    }
    purpose.value = "start";
    await prepare(request, request.viewOnly ? ["screen_recording"] : ["screen_recording", "accessibility"]);
  }

  /** Resume the original startup intent with the password entered in the share form. */
  async function submitTemporaryPassword(password: string): Promise<void> {
    const value = attempt;
    if (!value?.request || phase.value !== "awaiting_password" || work || !valid(value)
      || localVncServerPasswordValidationKey(password)) return;
    value.request.password = password;
    await run(value, () => retryTemporaryStart(value));
  }

  /** A manual permission guide shares the checks but never owns a start intent. */
  async function beginPermission(kind: LocalVncServerPermissionKind): Promise<void> {
    if (disposed || attempt || work) return;
    purpose.value = "permission";
    await prepare(null, [kind]);
  }

  /** Explicit maintenance replaces the host first, then guides missing grants without a start intent. */
  async function beginRepair(): Promise<void> {
    if (disposed || attempt || work) return;
    purpose.value = "repair";
    await prepare(null, ["screen_recording", "accessibility"], true);
  }

  async function prepare(request: BackgroundShareStartRequest | null, required: LocalVncServerPermissionKind[], replaceHost = false) {
    const value: Attempt = { request, required, expiresAt: 0, prompted: new Set(), recovered: false, cancelExpiry: null, allowTemporaryPassword: true };
    attempt = value;
    armExpiry(value);
    requiredPermissions.value = required;
    returnedToApp = false;
    phase.value = replaceHost ? "updating" : "preparing";
    options.setBusy(true);
    await run(value, async () => advance(value, await (replaceHost ? options.api.restart() : options.api.prepare())));
  }

  /** The dialog presents the current step before the user opens its system prompt. */
  async function requestPermission(openSettings = false): Promise<void> {
    const value = attempt;
    const kind = missingPermissions.value[0];
    if (!value || !kind || work || phase.value !== "awaiting_permission" || !valid(value)) return;
    await run(value, async () => {
      phase.value = "requesting_permission";
      options.setBusy(true);
      authorizing = true;
      try {
        const result = await options.api.permission(kind, openSettings);
        if (result.opened) value.prompted.add(kind);
        if (valid(value)) await advance(value, await options.readStatus());
      } finally { authorizing = false; }
    });
  }

  /** Called on return from system settings or explicit recheck; no permission polling. */
  async function resume(fromSettings = false): Promise<void> {
    const value = attempt;
    if (!value || phase.value !== "awaiting_permission" || !valid(value)) return;
    if (work) { returnedToApp ||= fromSettings; return work; }
    await run(value, async () => {
      options.setBusy(true);
      let overview = await options.readStatus();
      if (!valid(value)) return;
      const remaining = missing(overview, value);
      if (fromSettings && remaining[0] && value.prompted.has(remaining[0]) && !value.recovered && overview.server.status === "stopped") {
        value.recovered = true;
        phase.value = "recovering";
        overview = await options.api.prepare(true);
        if (!valid(value)) return;
      }
      await advance(value, overview);
    });
  }

  function cancel(): boolean {
    if (!attempt) return false;
    if (phase.value === "starting") {
      // An in-flight start cannot be undone here. Leaving cancels any later credential retry.
      attempt.allowTemporaryPassword = false;
      return false;
    }
    returnedToApp = false;
    finish(attempt);
    return true;
  }

  function dispose() {
    disposed = true;
    returnedToApp = false;
    if (attempt) finish(attempt);
  }

  return { phase, purpose, requiredPermissions, missingPermissions, signatureChangedPermissions, begin, submitTemporaryPassword, beginPermission, beginRepair, requestPermission, resume, cancel, dispose };
}
