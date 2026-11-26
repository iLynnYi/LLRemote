<script setup lang="ts">
import { computed, ref, toRef, watch } from "vue";
import { NButton, NCollapse, NCollapseItem, NSpace, useMessage } from "naive-ui";
import type { Translate } from "../../../app/types.ts";
import type { BackgroundShareOverview } from "../../../server/backgroundShare.ts";
import { useWindowsServiceSettingsFeedback } from "../../local-share/composables/useLocalShareFeedback.ts";
import {
  WINDOWS_SERVICE_SAS_POLICIES, WINDOWS_SERVICE_STATUSES,
  sharingRepairErrorKey, type WindowsServiceAction, type WindowsServiceOverview, type WindowsServiceRepairItem,
} from "../../../server/windowsService.ts";

const props = defineProps<{
  overview: WindowsServiceOverview;
  backgroundOverview?: BackgroundShareOverview | null;
  busy: boolean;
  repairing?: boolean;
  repairFocus?: number;
  repairItems?: WindowsServiceRepairItem[];
  running: boolean;
  backgroundRunning: boolean;
  active: boolean;
  t: Translate;
}>();
const emit = defineEmits<{
  action: [value: WindowsServiceAction | "refresh" | "setup"];
}>();
const message = useMessage();
const diagnostics = ref<string[]>([]);
const repairButton = ref<InstanceType<typeof NButton> | null>(null);
let focusedRepair = 0;
watch([() => props.active, () => props.busy, () => props.repairFocus, repairButton], ([active, busy, request, button]) => {
  if (!active || busy || !request || request === focusedRepair || !button) return;
  focusedRepair = request;
  const element = button.$el as HTMLButtonElement;
  element.focus({ preventScroll: true });
  element.scrollIntoView({ block: "nearest" });
}, { flush: "post" });
const { backgroundHint, sasDeniedHint } = useWindowsServiceSettingsFeedback({
  active: toRef(props, "active"),
  overview: toRef(props, "overview"),
  backgroundRunning: toRef(props, "backgroundRunning"),
  t: props.t,
  notify: (content, tone) => { message[tone](content, { duration: 2600, keepAliveOnHover: true }); },
});
const statusKey = computed(() => `localShare.windowsService.status.${WINDOWS_SERVICE_STATUSES.some(value => value === props.overview.status) ? props.overview.status : "unknown"}`);
const statusTone = computed(() => {
  if (props.overview.status === "running") return "success";
  if (["start_pending", "stop_pending"].includes(props.overview.status)) return "warning";
  return "muted";
});
const policyKey = computed(() => `localShare.windowsService.sasPolicy.${WINDOWS_SERVICE_SAS_POLICIES.some(value => value === props.overview.sasPolicy) ? props.overview.sasPolicy : "unknown"}`);
const inputKey = computed(() => `localShare.windowsService.inputStatus.${["ready", "waiting_for_frame", "injected", "blocked"].includes(props.overview.inputStatus ?? "") ? props.overview.inputStatus : "unknown"}`);
const sharingStateKey = computed(() => {
  const status = props.overview.installed ? props.overview.server?.status : props.backgroundOverview?.server.status;
  return status ? `localShare.serverStatus.${status}`
    : `localShare.background.currentSharingState.${props.overview.status === "stopped" ? "stopped" : "unknown"}`;
});
const sharingNeedsAttention = computed(() => props.overview.status === "running"
  && ["permission_missing", "port_conflict", "failed", "unknown"].includes(props.overview.server?.status ?? ""));
</script>

<template>
  <div class="windows-service-settings">
    <p v-if="backgroundHint" class="windows-service-hint" role="status">{{ backgroundHint }}</p>
    <div class="settings-group">
      <div class="settings-toggle settings-toggle-row">
        <span>
          <strong>{{ t('localShare.windowsService.serviceStatus') }}</strong>
          <small v-if="sharingNeedsAttention" class="sharing-startup-warning" role="alert">{{ t('localShare.background.currentSharing', { status: t(sharingStateKey) }) }}</small>
        </span>
        <div class="windows-service-actions">
          <span :class="['windows-service-state', `tone-${statusTone}`]" role="status">{{ t(statusKey) }}</span>
          <NButton size="small" :disabled="busy" @click="emit('action', 'refresh')">{{ t('localShare.refreshServerStatus') }}</NButton>
        </div>
      </div>

      <div class="settings-toggle settings-toggle-row">
        <span>
          <strong>{{ t('localShare.windowsService.repairTitle') }}</strong>
          <small>{{ t(overview.installed ? 'localShare.windowsService.repairHint' : 'localShare.windowsService.backgroundRepairHint') }}</small>
        </span>
        <NButton ref="repairButton" size="small" :loading="repairing" :disabled="busy" @click="emit('action', 'repair')">
          {{ t(repairing ? 'localShare.windowsService.repairing' : 'localShare.windowsService.repair') }}
        </NButton>
      </div>

      <div class="settings-toggle settings-toggle-row">
        <span>
          <strong>{{ t('localShare.windowsService.serviceMaintenance') }}</strong>
          <small>{{ t(overview.installed ? 'localShare.windowsService.serviceMaintenanceHint' : 'localShare.windowsService.serviceInstallHint') }}</small>
        </span>
        <NSpace size="small" class="windows-service-actions">
          <NButton v-if="!overview.installed" size="small" type="primary" :disabled="busy" @click="emit('action', 'setup')">
            {{ t('localShare.windowsService.goToInstall') }}
          </NButton>
          <template v-else>
            <NButton v-if="!overview.credentialConfigured || !overview.bindAddress || !overview.port" size="small" type="primary"
              :disabled="busy || running" @click="emit('action', 'setup')">{{ t('localShare.windowsService.goToInitialize') }}</NButton>
            <NButton v-else size="small" :disabled="busy || running || backgroundRunning" @click="emit('action', 'install')">
              {{ t('localShare.windowsService.update') }}
            </NButton>
            <NButton size="small" :disabled="busy || running" @click="emit('action', 'uninstall')">
              {{ t('localShare.windowsService.uninstall') }}
            </NButton>
          </template>
        </NSpace>
      </div>

      <div class="windows-service-diagnostics">
        <NCollapse v-model:expanded-names="diagnostics">
          <NCollapseItem name="service-diagnostics" :title="t('localShare.serviceDiagnostics')">
            <p class="windows-service-hint">{{ t('localShare.background.currentSharing', { status: t(sharingStateKey) }) }}</p>
            <p v-if="overview.installed" class="windows-service-hint">{{ t('localShare.windowsService.passwordManagementScope') }}</p>
            <p v-if="sasDeniedHint" class="windows-service-hint" role="status">{{ sasDeniedHint }}</p>
            <template v-if="repairItems?.length">
              <p class="windows-service-hint">{{ t('localShare.windowsService.repairDetails') }}</p>
              <dl class="settings-about-list windows-service-facts">
                <div v-for="item in repairItems" :key="item.kind">
                  <dt>{{ t(`localShare.windowsService.repairKinds.${item.kind}`) }}</dt>
                  <dd>{{ t(`localShare.windowsService.repairStatuses.${item.status}`) }}<template v-if="item.errorCode"> · {{ t(sharingRepairErrorKey(item.errorCode)) }}<br><code>{{ item.errorCode }}</code></template></dd>
                </div>
              </dl>
            </template>
            <dl v-if="overview.installed" class="settings-about-list windows-service-facts">
              <div><dt>{{ t('localShare.windowsService.title') }}</dt><dd>{{ t(statusKey) }}</dd></div>
              <div><dt>{{ t('localShare.windowsService.savedEndpoint') }}</dt><dd>{{ overview.bindAddress && overview.port ? `${overview.bindAddress}:${overview.port}` : t('localShare.windowsService.notConfigured') }}</dd></div>
              <div><dt>{{ t('localShare.windowsService.worker') }}</dt><dd>{{ t(overview.desktopRecovering ? 'localShare.windowsService.desktopRecovering' : overview.workerRunning ? 'localShare.windowsService.workerRunning' : 'localShare.windowsService.workerStopped') }}</dd></div>
              <div v-if="overview.userAgentRunning !== undefined"><dt>{{ t('localShare.windowsService.userAgent') }}</dt><dd>{{ t(overview.userAgentRunning ? 'localShare.windowsService.userAgentRunning' : 'localShare.windowsService.userAgentWaiting') }}</dd></div>
              <div><dt>{{ t('localShare.windowsService.session') }}</dt><dd>{{ overview.workerSessionId ?? '—' }}</dd></div>
              <div><dt>{{ t('localShare.windowsService.credential') }}</dt><dd>{{ t(overview.credentialConfigured ? 'localShare.windowsService.credentialReady' : 'localShare.windowsService.credentialMissing') }}</dd></div>
              <div><dt>{{ t('localShare.windowsService.passwordManagement') }}</dt><dd>{{ t(overview.passwordManagementAuthorized ? 'localShare.windowsService.passwordManagementGranted' : 'localShare.windowsService.passwordManagementHint') }}</dd></div>
              <div><dt>{{ t('localShare.windowsService.sas') }}</dt><dd>{{ t(policyKey) }}</dd></div>
              <div v-if="overview.desktop"><dt>{{ t('localShare.windowsService.desktop') }}</dt><dd>{{ overview.desktop }}</dd></div>
              <div v-if="overview.inputStatus"><dt>{{ t('localShare.windowsService.input') }}</dt><dd>{{ t(inputKey) }}</dd></div>
              <div v-if="overview.inputFailures !== undefined"><dt>{{ t('localShare.windowsService.inputFailures') }}</dt><dd>{{ overview.inputFailures }}</dd></div>
              <div v-if="overview.lastErrorCode"><dt>{{ t('localShare.runtimeError') }}</dt><dd><code>{{ overview.lastErrorCode }}</code></dd></div>
              <div v-if="overview.userAgentErrorCode"><dt>{{ t('localShare.windowsService.enhancementError') }}</dt><dd><code>{{ overview.userAgentErrorCode }}</code></dd></div>
              <div v-if="overview.sasErrorCode"><dt>{{ t('localShare.windowsService.sasError') }}</dt><dd><code>{{ overview.sasErrorCode }}</code></dd></div>
            </dl>
            <dl v-else class="settings-about-list windows-service-facts">
              <div><dt>{{ t('localShare.windowsService.repairKinds.background') }}</dt><dd>{{ t(!backgroundOverview ? 'localShare.unconfirmed' : backgroundOverview.processRunning ? 'localShare.windowsService.workerRunning' : 'localShare.windowsService.workerStopped') }}</dd></div>
              <div><dt>{{ t('localShare.windowsService.repairKinds.agent') }}</dt><dd>{{ t(`localShare.windowsService.repairStatuses.${!backgroundOverview || backgroundOverview.server.status !== 'running' ? 'deferred' : backgroundOverview.server.agent.status === 'running' ? 'healthy' : 'failed'}`) }}</dd></div>
              <div v-if="backgroundOverview?.lastErrorCode"><dt>{{ t('localShare.runtimeError') }}</dt><dd><code>{{ backgroundOverview.lastErrorCode }}</code></dd></div>
            </dl>
          </NCollapseItem>
        </NCollapse>
      </div>
    </div>
  </div>
</template>

<style scoped>
.windows-service-hint { color: #667281; font-size: 13px; line-height: 1.5; }
.windows-service-settings { display: grid; gap: 14px; min-width: 0; }
.windows-service-actions { display: flex; flex-wrap: wrap; align-items: center; justify-content: flex-end; gap: 12px; }
.windows-service-state { display: inline-flex; align-items: center; gap: 7px; font-size: 13px; font-weight: 700; white-space: nowrap; }
.windows-service-state::before { content: ""; width: 7px; height: 7px; border-radius: 50%; background: currentColor; }
.tone-success { color: #1f8a70; }
.tone-warning { color: #b7791f; }
.tone-muted { color: #667281; }
.windows-service-diagnostics { padding: 16px 20px; border-top: 1px solid #edf0f4; }
.windows-service-facts { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px 24px; padding: 4px 0; }
.windows-service-facts div { grid-template-columns: minmax(96px, 120px) minmax(0, 1fr); }
.windows-service-facts code { overflow-wrap: anywhere; white-space: pre-wrap; }

@media (max-width: 780px) {
  .windows-service-settings .settings-toggle-row { grid-template-columns: minmax(0, 1fr) auto; }
  .windows-service-actions { flex-wrap: wrap; max-width: 180px; }
  .windows-service-facts { grid-template-columns: minmax(0, 1fr); }
}
</style>
