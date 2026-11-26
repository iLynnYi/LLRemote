<script setup lang="ts">
import { computed } from "vue";
import { CloudUploadOutline } from "@vicons/ionicons5";
import { NBadge, NButton, NIcon } from "naive-ui";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";
import type { TransferTask } from "../../../session/transfers/fileTransferManager.ts";
import { summarizeTransfers } from "../../../session/transfers/fileTransferManager.ts";

const props = withDefaults(defineProps<{
  variant?: "rail" | "hud";
  tasksOverride?: TransferTask[];
  onActivate?: () => void;
}>(), {
  variant: "rail",
});

const {
  openGlobalTransferDrawer,
  sessionFileTransferManager,
  t,
} = useVncAppContext();

const summary = computed(() => props.tasksOverride
  ? summarizeTransfers(props.tasksOverride)
  : sessionFileTransferManager.summary.value);
const activeCount = computed(() => summary.value.queued + summary.value.transferring);
const badgeValue = computed(() => summary.value.failed || activeCount.value || summary.value.paused || undefined);
const badgeType = computed(() => summary.value.failed ? "error" : "info");
const statusLabel = computed(() => {
  if (summary.value.failed) {
    return t("sessions.fileTransfers.globalEntryFailed", { count: summary.value.failed });
  }
  if (activeCount.value) {
    return t("sessions.fileTransfers.globalEntryActive", { count: activeCount.value });
  }
  if (summary.value.paused) {
    return t("sessions.fileTransfers.globalEntryPaused", { count: summary.value.paused });
  }
  return t("sessions.fileTransfers.globalEntryIdle");
});
const entryLabel = computed(() => t("sessions.fileTransfers.globalEntryIdle"));
const statusClass = computed(() => [
  "global-transfer-status",
  props.variant === "rail" ? "rail-transfer-status" : "viewport-transfer-status",
]);
const buttonClass = computed(() => [
  "global-transfer-button",
  props.variant === "rail" ? "rail-item" : "remote-viewport-transfer-button",
]);

function activateTransferStatus() {
  if (props.onActivate) {
    props.onActivate();
    return;
  }
  openGlobalTransferDrawer();
}
</script>

<template>
  <NBadge
    :class="statusClass"
    :value="badgeValue"
    :type="badgeType"
    :show="Boolean(badgeValue)"
  >
    <NButton
      quaternary
      :class="buttonClass"
      :title="statusLabel"
      @click="activateTransferStatus"
    >
      <NIcon class="rail-glyph"><CloudUploadOutline /></NIcon>
      <strong v-if="props.variant === 'rail'">{{ entryLabel }}</strong>
    </NButton>
  </NBadge>
</template>
