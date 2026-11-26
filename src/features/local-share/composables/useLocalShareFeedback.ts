import { computed, watch, type Ref } from "vue";
import type { AppMessageTone, Translate } from "@app/types";
import { backgroundShareErrorKey } from "@server/backgroundShare";
import { windowsServiceErrorKey, type WindowsServiceOverview } from "@server/windowsService";
import type { useLocalShareWorkbench } from "./useLocalShareWorkbench.ts";

type Feedback = { id: string; content: string; tone: AppMessageTone };
type FeedbackOptions = {
  active: Readonly<Ref<boolean>>;
  notify: (content: string, tone: AppMessageTone) => void;
};
type LocalShareState = Pick<ReturnType<typeof useLocalShareWorkbench>,
  "sharingMode" | "sharingRuntimeError" | "backgroundShareOverview" | "windowsService"
  | "localSharePermissionCheckError" | "localVncServerOverview" | "localAgentStatusSummary"
  | "localVncServerMessage" | "localVncServerMessageTone">;

/** Unknown checks preserve delivery history; only confirmed recovery rearms a hint. */
function watchFeedback(options: FeedbackOptions, read: () => Feedback[] | null) {
  let previous = new Map<string, string>();
  watch(() => options.active.value ? read() : null, feedback => {
    if (!feedback) return;
    const next = new Map<string, string>();
    const delivered = new Set<string>();
    for (const item of feedback) {
      const identity = `${item.tone}:${item.content}`;
      next.set(item.id, identity);
      if (previous.get(item.id) !== identity && !delivered.has(identity)) {
        options.notify(item.content, item.tone);
        delivered.add(identity);
      }
    }
    previous = next;
  }, { immediate: true });
}

/** Mode guidance stays inline; failures use deduplicated messages. */
export function useLocalShareFeedback(options: FeedbackOptions & {
  state: LocalShareState;
  t: Translate;
}) {
  const { state, t } = options;
  const serviceNotInstalled = computed(() => !state.sharingRuntimeError.value
    && state.windowsService.overview.value?.supported === true
    && state.windowsService.overview.value.installed === false);
  const backgroundModeHint = computed(() => {
    const service = state.windowsService.overview.value;
    if (state.sharingRuntimeError.value || state.sharingMode.value !== "background" || !service?.supported) return "";
    return t(service.installed ? "localShare.existingBackgroundHint" : "localShare.windowsBackgroundHint");
  });
  const hints = computed(() => {
    const items: Feedback[] = [];
    const add = (id: string, key: string) => items.push({ id, content: t(key), tone: "info" });
    if (state.sharingMode.value === "windowsService") {
      if (state.backgroundShareOverview.value?.server.status === "running") add("running-background", "localShare.runningBackgroundHint");
      if (serviceNotInstalled.value) add("install", "localShare.windowsService.installHint");
    }
    return items;
  });
  watchFeedback(options, () => {
    const items: Feedback[] = [];
    const add = (id: string, content: string, tone: AppMessageTone = "warning") => items.push({ id, content, tone });
    const service = state.windowsService.overview.value;
    const background = state.backgroundShareOverview.value;
    const runtimeError = state.localSharePermissionCheckError.value || state.sharingRuntimeError.value;
    if (runtimeError) add("runtime", runtimeError, "error");
    const status = state.localVncServerOverview.value?.status;
    const actionError = state.localVncServerMessageTone.value === "error" && state.localVncServerMessage.value;
    const instanceError = state.sharingMode.value === "windowsService" ? service?.lastErrorCode : background?.lastErrorCode;
    // Prefer the specific error or the action's existing Message over a generic status hint.
    if (!runtimeError && !actionError && !instanceError && status
      && ["permission_missing", "port_conflict", "failed", "unknown"].includes(status)) {
      add("server-status", t(`localShare.serverStatusDetail.${status}`), status === "unknown" ? "warning" : "error");
    }
    if (state.sharingMode.value === "windowsService") {
      if (state.windowsService.endpointConflict.value) add("endpoint-conflict", t("localShare.windowsService.portConflict"));
      if (service?.lastErrorCode) add("service-error", t(windowsServiceErrorKey(service.lastErrorCode)), "error");
      if (service?.userAgentErrorCode) add("user-agent-error", t(windowsServiceErrorKey(service.userAgentErrorCode)));
    } else if (state.sharingMode.value === "background") {
      if (background?.lastErrorCode) add("background-error", t(backgroundShareErrorKey(background.lastErrorCode)));
    }
    if (state.localVncServerOverview.value?.agent.status === "failed") add("agent-error", t(state.localAgentStatusSummary.value.detailKey));
    return items;
  });
  return { serviceNotInstalled, backgroundModeHint, hints };
}

/** Both sharing pages use the same delivery history for permission identity warnings. */
export function useBackgroundPermissionRepairFeedback(options: FeedbackOptions & {
  state: Pick<ReturnType<typeof useLocalShareWorkbench>, "backgroundShareOverview" | "sharingRuntimeError"
    | "localSharePermissionCheckError" | "backgroundPermissionRepairNeeded">;
  t: Translate;
}) {
  const { state, t } = options;
  watchFeedback(options, () => {
    if (state.backgroundPermissionRepairNeeded.value) {
      return [{ id: "permission-identity", content: t("localShare.permissions.signatureChanged"), tone: "warning" }];
    }
    const background = state.backgroundShareOverview.value;
    if (state.sharingRuntimeError.value || state.localSharePermissionCheckError.value
      || !background?.processRunning || background.permissionSource === "pending"
      || background.server.permissions.some(permission => permission.status === "unknown")) return null;
    return [];
  });
}

/** Report leftover startup entries once while the sharing settings are visible. */
export function useSharingStartupSettingsFeedback(options: FeedbackOptions & Pick<LocalShareState,
  "sharingMode" | "backgroundShareOverview"> & { t: Translate }) {
  watchFeedback(options, () => {
    const background = options.backgroundShareOverview.value;
    if (options.sharingMode.value === null) return null;
    if (options.sharingMode.value === "background"
      && (!background || !background.startupRegistration || background.startupRegistration === "unknown")) return null;
    return options.sharingMode.value === "background" && background?.autoStart === false
      && background.startupRegistration === "enabled"
      ? [{ id: "startup-leftover", content: options.t("localShare.background.registration.leftover"), tone: "warning" }]
      : [];
  });
}

/** Settings feedback is scoped to the visible service category and diagnostics. */
export function useWindowsServiceSettingsFeedback(options: FeedbackOptions & {
  overview: Readonly<Ref<WindowsServiceOverview>>;
  backgroundRunning: Readonly<Ref<boolean>>;
  t: Translate;
}) {
  const backgroundHint = computed(() => options.backgroundRunning.value && options.overview.value.installed
    ? options.t("localShare.windowsService.stopBackgroundFirst") : "");
  const sasDeniedHint = computed(() => options.overview.value.installed && options.overview.value.sasPolicy === "services_denied"
    ? options.t("localShare.windowsService.sasDeniedHint") : "");
  watchFeedback(options, () => {
    const items: Feedback[] = [];
    const service = options.overview.value;
    if (service.sasErrorCode) items.push({ id: "sas-error", content: options.t("localShare.windowsService.sasLastFailure") + options.t(windowsServiceErrorKey(service.sasErrorCode)), tone: "warning" });
    return items;
  });
  return { backgroundHint, sasDeniedHint };
}
