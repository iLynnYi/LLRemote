<script setup lang="ts">
import { PlaySharp, StopSharp } from "@vicons/ionicons5";
import { NButton, NCard, NCheckbox, NIcon, NInput, NInputNumber, NSpace, useMessage } from "naive-ui";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";
import { computed, ref, watch } from "vue";
import { useLocalShareFeedback } from "../composables/useLocalShareFeedback.ts";
import LocalSharePermissionList from "./LocalSharePermissionList.vue";
import LocalShareWindowsFirewall from "./LocalShareWindowsFirewall.vue";
import { localShareConnectionFacts } from "../../../server/localShareConnectionDetails.ts";

const app = useVncAppContext();
const {
  activeWorkspace,
  copyLocalVncServerAddress,
  copyLocalVncServerConnectionProfile,
  copyLocalVncServerPort,
  disconnectLocalVncServerConnection,
  formatLocalVncServerConnectionDuration,
  formatLocalVncServerConnectionTime,
  localVncServerBindAddress,
  localVncServerCanCopyProfile,
  localVncServerCanToggle,
  localVncServerCopyProfileTitle,
  localVncServerDisplayBindAddress,
  localVncServerDisplayPort,
  localVncServerLoading,
  localVncServerOverview,
  localVncServerBindAddressInvalid,
  localVncServerPassword,
  localVncServerPasswordInvalid,
  localVncServerPort,
  localVncServerPortInvalid,
  localVncServerStartInputLocked,
  localVncServerToggleLabelKey,
  localVncServerToggleTitle,
  localVncServerViewOnly,
  refreshLocalVncServerStatus,
  requestLocalVncServerPermission,
  backgroundPermissionRepairNeeded, backgroundRuntimeOutdated, openSharingPermissionSettings,
  localShareServiceSetup, localShareSetupSavedPassword, localShareSetupPasswordRequired, localShareFallbackActive,
  cancelWindowsServiceSetup,
  localShareStartPending, localShareStartPhase, localShareStartChecking, cancelLocalShareStart,
  localSharePermissionChecking, localSharePermissionCheckError, inspectLocalSharePermissions,
  t,
  toggleLocalVncServer,
  sharingMode, sharingRuntimeError, backgroundShareOverview, windowsService,
} = app;
const message = useMessage();
const { hints } = useLocalShareFeedback({
  state: app,
  active: computed(() => activeWorkspace.value === "localShare"),
  t,
  notify: (content, tone) => { message[tone](content, { duration: 2600, keepAliveOnHover: true }); },
});
const sharingRunning = computed(() => !localShareServiceSetup.value && (sharingMode.value === 'windowsService'
  ? windowsService.running.value : localVncServerOverview.value?.status === 'running'));
const toggleBusy = computed(() => localVncServerLoading.value || windowsService.busy.value || localShareStartChecking.value
  || ['starting', 'stopping'].includes(localVncServerOverview.value?.status ?? '')
  || (sharingMode.value === 'windowsService' && ['start_pending', 'stop_pending'].includes(windowsService.overview.value?.status ?? '')));
const serviceSelected = computed(() => localShareServiceSetup.value || windowsService.overview.value?.installed === true);
const sharingModeLabel = computed(() => {
  if (localShareFallbackActive.value) return "localShare.windowsService.fallbackMode";
  if (localVncServerOverview.value?.status !== "running") return "";
  if (sharingMode.value === "background" && backgroundShareOverview.value?.temporaryShare) return "localShare.temporaryPassword.badge";
  if (windowsService.overview.value?.supported !== true) return "";
  if (sharingMode.value === "windowsService") return "localShare.windowsService.serviceMode";
  return sharingMode.value === "background" ? "localShare.windowsService.backgroundMode" : "";
});
// Common fields bind to the selected instance and its start flow.
const formPort = computed({
  get: () => serviceSelected.value ? windowsService.port.value : localVncServerPort.value,
  set: value => { if (serviceSelected.value) windowsService.port.value = value; else localVncServerPort.value = value; },
});
const formBindAddress = computed({
  get: () => serviceSelected.value ? windowsService.bindAddress.value : localVncServerBindAddress.value,
  set: value => { if (serviceSelected.value) windowsService.bindAddress.value = value; else localVncServerBindAddress.value = value; },
});
const formPassword = computed({
  get: () => serviceSelected.value ? windowsService.password.value : localVncServerPassword.value,
  set: value => { if (serviceSelected.value) windowsService.password.value = value; else localVncServerPassword.value = value; },
});
const formViewOnly = computed({
  get: () => serviceSelected.value ? windowsService.viewOnly.value : localVncServerViewOnly.value,
  set: value => { if (serviceSelected.value) windowsService.viewOnly.value = value; else localVncServerViewOnly.value = value; },
});
const formLocked = computed(() => localShareServiceSetup.value ? localVncServerLoading.value || windowsService.busy.value : localVncServerStartInputLocked.value || (serviceSelected.value
  && (windowsService.busy.value || windowsService.running.value || !windowsService.overview.value?.installed)));
const formPortInvalid = computed(() => serviceSelected.value ? windowsService.portInvalid.value : localVncServerPortInvalid.value);
const formBindAddressInvalid = computed(() => serviceSelected.value ? windowsService.bindAddressInvalid.value : localVncServerBindAddressInvalid.value);
const formPasswordInvalid = computed(() => serviceSelected.value
  ? Boolean(formPassword.value && !windowsService.passwordValid.value) : localVncServerPasswordInvalid.value);
const awaitingTemporaryPassword = computed(() => localShareStartPhase.value === "awaiting_password");
const passwordLocked = computed(() => awaitingTemporaryPassword.value
  ? localVncServerLoading.value || sharingMode.value !== "background"
  : formLocked.value);
const passwordSaved = computed(() => serviceSelected.value
  ? localShareServiceSetup.value ? localShareSetupSavedPassword.value : Boolean(windowsService.overview.value?.credentialConfigured)
  : Boolean(backgroundShareOverview.value?.credentialConfigured));
// A fixed placeholder indicates a saved secret without reading or submitting it.
const passwordPlaceholder = computed(() => passwordSaved.value && !awaitingTemporaryPassword.value
  ? '••••••••' : t('localShare.accessPasswordPlaceholder'));
const runtimeUnavailable = computed(() => sharingMode.value === null || Boolean(sharingRuntimeError.value));
const permissionUnavailable = computed(() => runtimeUnavailable.value || Boolean(localSharePermissionCheckError.value));
const connectionFacts = computed(() => localShareConnectionFacts({
  overview: localVncServerOverview.value,
  runtimeUnavailable: runtimeUnavailable.value,
  permissionUnavailable: permissionUnavailable.value,
  service: sharingMode.value === "windowsService" ? windowsService.overview.value : null,
}));
const passwordInput = ref<InstanceType<typeof NInput> | null>(null);
watch([activeWorkspace, awaitingTemporaryPassword, localShareSetupPasswordRequired, passwordLocked], ([workspace, temporary, setupRequired, locked]) => {
  if (workspace === "localShare" && !locked && (temporary || (setupRequired && localShareServiceSetup.value))) {
    passwordInput.value?.focus();
  }
}, { flush: "post" });
const firewall = ref<InstanceType<typeof LocalShareWindowsFirewall> | null>(null);
async function refreshSharingStatus() {
  await refreshLocalVncServerStatus();
  await firewall.value?.refresh();
}
watch(activeWorkspace, workspace => {
  if (workspace !== "localShare") cancelWindowsServiceSetup();
  if (workspace === "localShare") void inspectLocalSharePermissions();
  windowsService.password.value = "";
  localVncServerPassword.value = "";
}, { immediate: true });
</script>

<template>
<section v-show="activeWorkspace === 'localShare'" class="local-share-workbench">
  <header class="local-share-header">
    <div class="local-share-heading">
      <h2>{{ t("localShare.title") }}</h2>
      <span v-if="sharingModeLabel" class="local-share-mode-label">{{ t(sharingModeLabel) }}</span>
    </div>
    <NSpace class="local-share-actions" align="center">
      <NButton
        class="local-share-toggle-button"
        :class="{ 'local-share-temporary-start-button': awaitingTemporaryPassword }"
        :type="sharingRunning ? 'error' : 'primary'"
        :secondary="sharingRunning || localShareStartPhase === 'awaiting_permission'"
        :loading="toggleBusy"
        :disabled="!localVncServerCanToggle || toggleBusy"
        :aria-busy="toggleBusy"
        :aria-label="t(localVncServerToggleLabelKey)"
        :title="localVncServerToggleTitle"
        @click="toggleLocalVncServer"
      >
        <template v-if="!toggleBusy" #icon>
          <NIcon :component="sharingRunning || (localShareStartPending && !awaitingTemporaryPassword) ? StopSharp : PlaySharp" aria-hidden="true" />
        </template>
        <template v-if="awaitingTemporaryPassword">{{ t(localVncServerToggleLabelKey) }}</template>
      </NButton>
      <NButton
        circle
        class="local-share-icon-button"
        :disabled="localVncServerLoading"
        :aria-label="t('localShare.refreshServerStatus')"
        :title="t('localShare.refreshServerStatus')"
        @click="refreshSharingStatus"
      >
        ↻
      </NButton>
      <NButton
        circle
        class="local-share-icon-button"
        :disabled="!localVncServerCanCopyProfile"
        :aria-label="t('localShare.copyServerProfile')"
        :title="localVncServerCopyProfileTitle"
        @click="copyLocalVncServerConnectionProfile"
      >
        ⧉
      </NButton>
    </NSpace>
  </header>

  <p v-for="hint in hints" :key="hint.id" class="local-share-server-message" role="status">{{ hint.content }}</p>
  <div v-if="backgroundRuntimeOutdated" class="local-share-mode-notice" role="status">
    <span>{{ t('localShare.permissions.outdated') }}</span>
    <NButton text type="primary" size="small" @click="openSharingPermissionSettings">{{ t('localShare.permissions.updateAndRepair') }}</NButton>
  </div>

  <NCard class="local-share-server-card" :bordered="true">
    <div class="local-share-server-form">
      <label :class="{ invalid: formPortInvalid }">
        {{ t("toolbar.port") }}
        <NInputNumber
          v-model:value="formPort"
          :min="1"
          :max="65535"
          :precision="0"
          :step="1"
          :status="formPortInvalid ? 'error' : undefined"
          :aria-invalid="formPortInvalid"
          :disabled="formLocked"
        />
      </label>
      <label :class="{ invalid: formBindAddressInvalid }">
        {{ t("localShare.bindAddress") }}
        <NInput
          v-model:value="formBindAddress"
          autocomplete="off"
          spellcheck="false"
          :status="formBindAddressInvalid ? 'error' : undefined"
          :aria-invalid="formBindAddressInvalid"
          :disabled="formLocked"
        />
      </label>
      <label :class="{ invalid: formPasswordInvalid }">
        {{ t("localShare.accessPassword") }}
        <NInput
          ref="passwordInput"
          v-model:value="formPassword"
          type="password"
          show-password-on="click"
          autocomplete="new-password"
          :placeholder="passwordPlaceholder"
          :status="formPasswordInvalid ? 'error' : undefined"
          :aria-invalid="formPasswordInvalid"
          :input-props="awaitingTemporaryPassword ? { 'aria-describedby': 'local-share-temporary-password-hint' } : undefined"
          :disabled="passwordLocked"
        />
      </label>
      <label class="local-share-server-checkbox">
        <NCheckbox v-model:checked="formViewOnly" :disabled="formLocked" />
        {{ t("localShare.viewOnly") }}
      </label>
      <div v-if="awaitingTemporaryPassword" class="local-share-temporary-password-notice">
        <small id="local-share-temporary-password-hint" role="status">
          {{ t(formPasswordInvalid ? 'localShare.accessPasswordTooLong' : 'localShare.temporaryPassword.prompt') }}
        </small>
        <NButton text size="small" @click="cancelLocalShareStart">{{ t('localShare.permissions.cancelStart') }}</NButton>
      </div>
    </div>
    <p v-if="runtimeUnavailable" class="local-share-status-note">{{ t(localVncServerOverview ? 'localShare.lastConfirmedStatus' : 'localShare.statusPending') }}</p>
    <div v-if="runtimeUnavailable || localVncServerOverview" class="local-share-addresses local-share-server-addresses">
      <span>{{ t("localShare.connectionAddresses") }}</span>
      <p v-if="runtimeUnavailable" class="local-share-inline-unconfirmed">{{ t('localShare.addressesUnconfirmed') }}</p>
      <div v-else>
        <NButton
          v-for="address in localVncServerOverview?.addresses ?? []"
          :key="`${address.kind}-${address.address}`"
          size="small"
          class="local-share-address-tag"
          :title="t('localShare.copyAddress')"
          @click="copyLocalVncServerAddress(address.address)"
        >
          <code>{{ address.label }} · {{ address.address }}</code>
        </NButton>
        <NButton
          size="small"
          class="local-share-address-tag"
          :title="t('localShare.copyPort')"
          @click="copyLocalVncServerPort"
        >
          <code>{{ t('localShare.connectionPort') }}: {{ localVncServerDisplayPort }}</code>
        </NButton>
      </div>
    </div>
    <div class="local-share-connections">
      <div class="local-share-panel-head local-share-connections-head">
        <h3>{{ t("localShare.connectionDevices.title") }}</h3>
        <span>{{ runtimeUnavailable ? '—' : (localVncServerOverview?.activeConnections ?? '—') }}</span>
      </div>
      <div
        v-if="runtimeUnavailable || !localVncServerOverview"
        class="local-share-connections-empty"
      >
        {{ t('localShare.connectionDevices.unconfirmed') }}
      </div>
      <div
        v-else-if="!localVncServerOverview.activeConnectionDevices.length"
        class="local-share-connections-empty"
      >
        {{ t("localShare.connectionDevices.empty") }}
      </div>
      <div v-else class="local-share-connection-list">
        <article
          v-for="connection in localVncServerOverview.activeConnectionDevices"
          :key="connection.id"
          class="local-share-connection-device"
        >
          <strong class="local-share-connection-address" :title="connection.ipAddress || undefined">
            {{ connection.ipAddress || "-" }}
          </strong>
          <NButton
            size="small"
            type="error"
            secondary
            :disabled="runtimeUnavailable || localVncServerLoading || connection.status === 'closing'"
            @click="disconnectLocalVncServerConnection(connection)"
          >
            {{ t("localShare.connectionDevices.disconnect") }}
          </NButton>
          <p class="local-share-connection-meta">
            <span>{{ t("localShare.connectionDevices.durationValue", { duration: formatLocalVncServerConnectionDuration(connection) }) }}</span>
            <span class="local-share-connection-started-at" :title="formatLocalVncServerConnectionTime(connection)">
              {{ t("localShare.connectionDevices.connectedAtValue", { time: formatLocalVncServerConnectionTime(connection, true) }) }}
            </span>
          </p>
        </article>
      </div>
    </div>
    <LocalShareWindowsFirewall
      v-if="windowsService.overview.value?.supported === true"
      ref="firewall"
      :active="activeWorkspace === 'localShare'"
      :t="t"
    />
    <div v-else-if="windowsService.overview.value?.supported === false" class="local-share-permissions">
      <div class="local-share-panel-head">
        <h3>{{ t("localShare.permissions.title") }}</h3>
        <NButton v-if="backgroundPermissionRepairNeeded" text type="primary" size="small" @click="openSharingPermissionSettings">{{ t('localShare.permissions.openRepairSettings') }}</NButton>
      </div>
      <div v-if="localSharePermissionChecking" class="local-share-connections-empty">{{ t('localShare.permissions.startPhases.checking') }}</div>
      <div v-else-if="permissionUnavailable || !localVncServerOverview?.permissions.length" class="local-share-connections-empty">
        {{ t(permissionUnavailable ? 'localShare.permissions.checkUnavailable' : 'localShare.permissions.pending') }}
      </div>
      <LocalSharePermissionList
        v-else
        :permissions="localVncServerOverview?.permissions ?? []"
        :busy="localVncServerLoading || localShareStartPending"
        :view-only="formViewOnly"
        :t="t"
        @request="requestLocalVncServerPermission"
      />
    </div>
    <details class="local-share-connection-details">
      <summary>{{ t('localShare.connectionDetails') }}</summary>
      <dl class="local-share-server-facts">
        <div>
          <dt>{{ t('localShare.bindAddress') }}</dt>
          <dd>{{ runtimeUnavailable ? (localVncServerOverview?.bindAddress ?? '—') : localVncServerDisplayBindAddress }}</dd>
        </div>
        <div>
          <dt>{{ t('localShare.agentPort') }}</dt>
          <dd>{{ runtimeUnavailable ? t('localShare.unconfirmed') : (localVncServerOverview?.agent.port ?? '—') }}</dd>
        </div>
        <div>
          <dt>{{ t('localShare.securityType') }}</dt>
          <dd>{{ !localVncServerOverview ? t('localShare.unconfirmed') : localVncServerOverview.securityType === 'vnc_auth' ? t('localShare.vncPasswordAuthentication') : localVncServerOverview.securityType }}</dd>
        </div>
        <div v-for="fact in connectionFacts" :key="fact.id" :data-connection-fact="fact.id">
          <dt>{{ t(fact.labelKey) }}</dt>
          <dd>
            <span>{{ t(fact.valueKey) }}</span>
            <small v-if="fact.reasonKey">{{ t(fact.reasonKey) }}</small>
          </dd>
        </div>
      </dl>
    </details>
  </NCard>

</section>
</template>

<style scoped>
.local-share-header { flex-wrap: wrap; gap: 12px; }
.local-share-heading { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; }
.local-share-mode-label { padding: 3px 8px; border-radius: 6px; background: #edf5f2; color: #397261; font-size: 12px; line-height: 1.5; }
.local-share-toggle-button.n-button { width: 34px; height: 34px; min-width: 34px; padding: 0; font-size: 13px; }
.local-share-temporary-start-button.n-button { width: auto; padding: 0 12px; }
.local-share-server-facts small { display: block; margin-top: 4px; color: #667281; font-size: 11px; font-weight: 400; line-height: 1.5; white-space: normal; overflow-wrap: anywhere; }
</style>
