<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { NButton, NModal, NSpace, useThemeVars } from "naive-ui";
import type { Translate } from "../../../app/types.ts";
import { backgroundShareErrorKey } from "../../../server/backgroundShare.ts";

const props = defineProps<{
  busy: boolean;
  canMaintain: boolean;
  outdated: boolean;
  permissionRepairNeeded: boolean;
  checkError: string;
  runtimeErrorCode: string | null;
  active: boolean;
  repairFocus: number;
  t: Translate;
}>();
const emit = defineEmits<{
  restart: [];
  repair: [];
}>();
const theme = useThemeVars();
const confirmation = ref<"restart" | "repair">("restart");
const confirmationVisible = ref(false);
const confirmingRepair = computed(() => confirmation.value === "repair");
watch(() => props.active, active => { if (!active) confirmationVisible.value = false; });
function openConfirmation(action: "restart" | "repair") {
  confirmation.value = action;
  confirmationVisible.value = true;
}
function confirmMaintenance() {
  if (!confirmationVisible.value || props.busy || !props.canMaintain || !props.active) return;
  const action = confirmation.value;
  confirmationVisible.value = false;
  if (action === "restart") emit("restart");
  else emit("repair");
}
const repairSection = ref<HTMLElement | null>(null);
const troubleshootingOpen = ref(false);
async function focusRepair() {
  troubleshootingOpen.value = true;
  await nextTick();
  if (!props.active) return;
  repairSection.value?.focus({ preventScroll: true });
  repairSection.value?.scrollIntoView({ block: "center", behavior: "smooth" });
}
let focusedRequest = 0;
watch([() => props.active, () => props.repairFocus], async ([active, request]) => {
  if (!active || !request || request === focusedRequest) return;
  focusedRequest = request;
  await focusRepair();
}, { immediate: true });
const restartIssue = computed(() => props.checkError
  || (props.runtimeErrorCode ? props.t(backgroundShareErrorKey(props.runtimeErrorCode)) : ""));
const needsRepair = computed(() => props.outdated || props.permissionRepairNeeded);
</script>

<template>
  <div class="settings-group mac-sharing-settings">
    <details class="mac-sharing-troubleshooting" :open="troubleshootingOpen" @toggle="troubleshootingOpen = ($event.target as HTMLDetailsElement).open">
      <summary>{{ t('localShare.permissions.troubleshooting') }}</summary>
      <div class="settings-toggle settings-toggle-row">
        <span>
          <strong>{{ t('localShare.permissions.restart') }}</strong>
          <small v-if="restartIssue" class="maintenance-issue" role="status">{{ restartIssue }}</small>
        </span>
        <NButton size="small" :disabled="busy || !canMaintain" @click="openConfirmation('restart')">{{ t('localShare.permissions.confirmRestart') }}</NButton>
      </div>
      <div ref="repairSection" class="settings-toggle settings-toggle-row repair-section" tabindex="-1">
        <span>
          <strong>{{ t('localShare.permissions.updateAndRepair') }}</strong>
          <small v-if="outdated" class="maintenance-issue" role="status">{{ t('localShare.permissions.outdatedHint') }}</small>
        </span>
        <NButton size="small" :type="needsRepair ? 'primary' : 'default'" :secondary="needsRepair" :disabled="busy || !canMaintain" @click="openConfirmation('repair')">{{ t('localShare.permissions.confirmRepair') }}</NButton>
      </div>
    </details>
  </div>
  <NModal
    :show="confirmationVisible && active"
    preset="card"
    :title="t(confirmingRepair ? 'localShare.permissions.updateAndRepair' : 'localShare.permissions.restart')"
    size="small"
    :bordered="false"
    :mask-closable="false"
    style="width: min(420px, calc(100vw - 32px))"
    :header-style="{ padding: '18px 20px 12px', fontSize: '16px' }"
    :content-style="{ padding: '0 20px 16px' }"
    :footer-style="{ padding: '0 20px 18px' }"
    @update:show="show => confirmationVisible = show"
  >
    <div class="maintenance-confirmation">
      <p>{{ t(confirmingRepair ? 'localShare.permissions.updateAndRepairConfirm' : 'localShare.permissions.restartConfirm') }}</p>
      <p v-if="confirmingRepair">{{ t('localShare.permissions.repairPermissionHint') }}</p>
      <p class="maintenance-confirmation-note">{{ t('localShare.permissions.maintenanceAfter') }}</p>
    </div>
    <template #footer>
      <NSpace justify="end" :size="8">
        <NButton size="small" @click="confirmationVisible = false">{{ t('localShare.permissions.cancel') }}</NButton>
        <NButton size="small" type="primary" :disabled="busy || !canMaintain" @click="confirmMaintenance">
          {{ t(confirmingRepair ? 'localShare.permissions.confirmRepair' : 'localShare.permissions.confirmRestart') }}
        </NButton>
      </NSpace>
    </template>
  </NModal>
</template>

<style scoped>
.mac-sharing-settings { min-width: 0; }
.mac-sharing-troubleshooting summary { padding: 16px 20px; cursor: pointer; font-size: 14px; color: v-bind('theme.textColor2'); }
.mac-sharing-troubleshooting summary:focus-visible { outline: 2px solid v-bind('theme.primaryColor'); outline-offset: -2px; }
.settings-toggle .maintenance-issue { color: #a64224; overflow-wrap: anywhere; }
.maintenance-confirmation p { margin: 0; font-size: 13px; line-height: 1.65; overflow-wrap: anywhere; }
.maintenance-confirmation p + p { margin-top: 10px; }
.maintenance-confirmation-note { color: v-bind('theme.textColor3'); }
.repair-section:focus-visible { outline: 2px solid v-bind('theme.primaryColor'); outline-offset: -2px; }
@media (max-width: 780px) {
  .mac-sharing-settings .settings-toggle-row { grid-template-columns: minmax(0, 1fr); }
  .mac-sharing-settings .settings-toggle-row > .n-button { justify-self: start; }
}
</style>
