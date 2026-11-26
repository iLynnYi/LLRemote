<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { CloseOutline, PauseOutline, PlayOutline, RefreshOutline, TrashOutline, CopyOutline, FolderOpenOutline } from "@vicons/ionicons5";
import { NButton, NDrawer, NDrawerContent, NEmpty, NIcon, NModal, NProgress, NSelect } from "naive-ui";
import { useI18n } from "vue-i18n";
import { resolveTransferBatchAction, resolveTransferHistorySelection, type TransferBatchAction, type TransferTaskPathAction } from "../../../session/transfers/fileTransferTaskActions.ts";
import type { TransferState } from "../../../session/transfers/fileTransferManager.ts";
import { fileTransferErrorMessageKey } from "../../../session/transfers/fileTransferErrors.ts";

export type TransferTaskView = {
  id: string; sessionId: string; target: string; fileName: string; state: TransferState;
  phase?: "preparing" | "transferring" | "verifying";
  transferredBytes: number; totalBytes: number; progress: number; speedBytesPerSecond: number;
  etaSeconds: number | null; errorMessage?: string; remotePath?: string;
};
type TransferFilter = "all" | TransferState;
type TaskAction = "pause" | "resume" | "retry" | "cancel";
const props = withDefaults(defineProps<{
  show: boolean; title: string; tasks: TransferTaskView[]; emptyDescription: string;
  sessionOptions?: Array<{ label: string; value: string }>; selectedSessionId?: string | null;
  showSessionFilter?: boolean; canOpenRemoteDirectory?: (sessionId: string) => boolean;
  clearingHistory?: boolean;
}>(), { sessionOptions: () => [], selectedSessionId: null, showSessionFilter: false, clearingHistory: false });
const emit = defineEmits<{
  "update:show": [value: boolean]; "update:selectedSessionId": [value: string | null];
  "task-action": [action: TaskAction, taskId: string]; "clear-history": [taskIds: string[]];
  "batch-action": [action: TransferBatchAction, taskIds: string[]];
  "path-action": [action: TransferTaskPathAction, sessionId: string, taskId: string];
}>();
const { t, locale } = useI18n();
const activeFilter = ref<TransferFilter>("all");
const page = ref(0);
const pageSize = 100;
const filters: TransferFilter[] = ["all", "transferring", "queued", "paused", "failed", "completed", "skipped", "canceled"];
const scopedTasks = computed(() => props.tasks.filter(task => props.selectedSessionId === null || task.sessionId === props.selectedSessionId));
const filteredTasks = computed(() => scopedTasks.value.filter(task => activeFilter.value === "all" || task.state === activeFilter.value));
const pageCount = computed(() => Math.max(1, Math.ceil(filteredTasks.value.length / pageSize)));
const visibleTasks = computed(() => filteredTasks.value.slice(page.value * pageSize, (page.value + 1) * pageSize));
watch([activeFilter, () => props.selectedSessionId], () => { page.value = 0; });
watch(pageCount, count => { if (page.value >= count) page.value = count - 1; });
const batchAction = computed(() => resolveTransferBatchAction(filteredTasks.value));
const hasFilter = computed(() => activeFilter.value !== "all" || props.selectedSessionId !== null);
const historySelection = computed(() => resolveTransferHistorySelection(filteredTasks.value));
const pendingHistorySelection = ref<ReturnType<typeof resolveTransferHistorySelection> | null>(null);
const historyConfirmationVisible = ref(false);
watch([() => props.show, activeFilter, () => props.selectedSessionId], () => { historyConfirmationVisible.value = false; });
function requestHistoryClear() {
  if (props.clearingHistory || !historySelection.value.taskIds.length) return;
  if (historySelection.value.pausedCount) {
    pendingHistorySelection.value = historySelection.value;
    historyConfirmationVisible.value = true;
  }
  else emit("clear-history", historySelection.value.taskIds);
}
function confirmHistoryClear() {
  if (!props.show || props.clearingHistory || !historyConfirmationVisible.value || !pendingHistorySelection.value) return;
  emit("clear-history", pendingHistorySelection.value.taskIds);
  historyConfirmationVisible.value = false;
}
function selectedTaskIds() { return filteredTasks.value.map(task => task.id); }
function controlBatch() { if (batchAction.value) emit("batch-action", batchAction.value, selectedTaskIds()); }
function filterCount(filter: TransferFilter) { return filter === "all" ? scopedTasks.value.length : scopedTasks.value.filter(task => task.state === filter).length; }
function canPause(task: TransferTaskView) { return task.state === "queued" || task.state === "transferring"; }
function canResume(task: TransferTaskView) { return task.state === "paused"; }
function canRetry(task: TransferTaskView) { return task.state === "failed" || task.state === "canceled"; }
function canCancel(task: TransferTaskView) { return task.state === "queued" || task.state === "transferring" || task.state === "paused"; }
function formatBytes(bytes: number) { if (bytes <= 0) return "0 B"; const units = ["B", "KB", "MB", "GB"]; let value = bytes, index = 0; while (value >= 1024 && index < units.length - 1) { value /= 1024; index += 1; } return `${value >= 10 || index === 0 ? Math.round(value) : value.toFixed(1)} ${units[index]}`; }
function formatSpeed(value: number) { return value > 0 ? `${formatBytes(value)}/s` : "-"; }
function formatEta(value: number | null) { if (value === null) return "-"; if (value <= 0) return "00:00"; return `${String(Math.floor(value / 60)).padStart(2, "0")}:${String(value % 60).padStart(2, "0")}`; }
function formatError(value: string) { return t(fileTransferErrorMessageKey(value)); }
function stateLabel(task: TransferTaskView) { return t(task.state === "transferring" && task.phase
  ? `sessions.fileTransfers.phases.${task.phase}` : `sessions.fileTransfers.states.${task.state}`); }
</script>
<template>
  <NDrawer :show="show" placement="right" :width="460" @update:show="value => emit('update:show', value)">
    <NDrawerContent class="global-transfer-drawer" :title="title">
      <NSelect v-if="showSessionFilter" :value="selectedSessionId" class="global-transfer-session-filter" :options="sessionOptions" clearable :placeholder="t('sessions.fileTransfers.allSessions')" @update:value="value => emit('update:selectedSessionId', value || null)" />
      <nav class="global-transfer-filters" :aria-label="t('sessions.fileTransfers.globalTitle')"><NButton v-for="filter in filters" :key="filter" size="small" :type="activeFilter === filter ? 'primary' : 'default'" secondary @click="activeFilter = filter">{{ t(`sessions.fileTransfers.filters.${filter}`) }} {{ filterCount(filter) }}</NButton></nav>
      <section v-if="filteredTasks.length" class="global-transfer-list"><article v-for="task in visibleTasks" :key="task.id" v-memo="[task.state, task.phase, task.fileName, task.target, task.progress, task.transferredBytes, task.totalBytes, task.speedBytesPerSecond, task.etaSeconds, task.errorMessage, task.remotePath, task.state === 'completed' && canOpenRemoteDirectory?.(task.sessionId), locale]" class="global-transfer-row">
        <div class="global-transfer-row-head"><strong :title="task.fileName">{{ task.fileName }}</strong><span :class="['global-transfer-state', `state-${task.state}`]">{{ stateLabel(task) }}</span></div>
        <div class="global-transfer-meta"><span>{{ t('sessions.fileTransfers.fields.target') }} {{ task.target }}</span><span>{{ formatBytes(task.transferredBytes) }} / {{ formatBytes(task.totalBytes) }}</span></div>
        <NProgress type="line" :percentage="task.progress" :height="6" :border-radius="3" :show-indicator="false" :status="task.state === 'failed' ? 'error' : task.state === 'completed' ? 'success' : 'default'" />
        <div class="global-transfer-meta"><span>{{ t('sessions.fileTransfers.fields.speed') }} {{ formatSpeed(task.speedBytesPerSecond) }}</span><span>{{ t('sessions.fileTransfers.fields.eta') }} {{ formatEta(task.etaSeconds) }}</span></div>
        <div v-if="task.state === 'completed' && task.remotePath" class="global-transfer-actions">
          <NButton size="tiny" secondary @click="emit('path-action', 'copy-path', task.sessionId, task.id)"><template #icon><NIcon><CopyOutline /></NIcon></template>{{ t('sessions.fileTransfers.copyRemotePath') }}</NButton>
          <NButton size="tiny" secondary :disabled="!canOpenRemoteDirectory?.(task.sessionId)" @click="emit('path-action', 'open-directory', task.sessionId, task.id)"><template #icon><NIcon><FolderOpenOutline /></NIcon></template>{{ t('sessions.toolbox.uploadedFileActions.revealFile') }}</NButton>
        </div>
        <p v-if="task.errorMessage" :class="['global-transfer-error', { 'global-transfer-notice': task.state === 'skipped' }]">{{ formatError(task.errorMessage) }}</p>
        <details v-if="task.errorMessage && task.state === 'failed'" class="global-transfer-error-details">
          <summary>{{ t('sessions.fileTransfers.errorDetails') }}</summary>
          <code>{{ task.errorMessage }}</code>
        </details>
        <div class="global-transfer-actions"><NButton v-if="canPause(task)" size="tiny" secondary @click="emit('task-action', 'pause', task.id)"><template #icon><NIcon><PauseOutline /></NIcon></template>{{ t('sessions.fileTransfers.pause') }}</NButton><NButton v-if="canResume(task)" size="tiny" secondary @click="emit('task-action', 'resume', task.id)"><template #icon><NIcon><PlayOutline /></NIcon></template>{{ t('sessions.fileTransfers.resume') }}</NButton><NButton v-if="canRetry(task)" size="tiny" secondary @click="emit('task-action', 'retry', task.id)"><template #icon><NIcon><RefreshOutline /></NIcon></template>{{ t('sessions.fileTransfers.retry') }}</NButton><NButton v-if="canCancel(task)" size="tiny" secondary type="error" @click="emit('task-action', 'cancel', task.id)"><template #icon><NIcon><CloseOutline /></NIcon></template>{{ t('sessions.fileTransfers.cancel') }}</NButton></div>
      </article></section>
      <NEmpty v-else class="global-transfer-empty" :description="emptyDescription" />
      <div v-if="pageCount > 1" class="global-transfer-pages">
        <NButton size="small" secondary :disabled="page === 0" @click="page -= 1">{{ t('sessions.fileTransfers.previousPage') }}</NButton>
        <span>{{ t('sessions.fileTransfers.pageInfo', { current: page + 1, total: pageCount }) }}</span>
        <NButton size="small" secondary :disabled="page + 1 >= pageCount" @click="page += 1">{{ t('sessions.fileTransfers.nextPage') }}</NButton>
      </div>
      <template #footer>
        <div class="global-transfer-footer">
          <NButton v-if="batchAction" secondary size="small" :type="batchAction === 'pause' ? 'warning' : 'success'" @click="controlBatch">
            <template #icon><NIcon><PauseOutline v-if="batchAction === 'pause'" /><PlayOutline v-else /></NIcon></template>
            {{ t(`sessions.fileTransfers.${batchAction === 'pause' ? (hasFilter ? 'pauseFiltered' : 'pauseAll') : (hasFilter ? 'resumeFiltered' : 'resumeAll')}`) }}
          </NButton>
          <NButton secondary size="small" :loading="clearingHistory" :disabled="clearingHistory || !historySelection.taskIds.length" @click="requestHistoryClear"><template #icon><NIcon><TrashOutline /></NIcon></template>{{ t(`sessions.fileTransfers.${clearingHistory ? 'clearingHistory' : hasFilter ? 'clearFilteredHistory' : 'clearHistory'}`) }}</NButton>
          <NButton size="small" @click="emit('update:show', false)">{{ t('sessions.fileTransfers.close') }}</NButton>
        </div>
      </template>
    </NDrawerContent>
  </NDrawer>
  <NModal
    :show="show && historyConfirmationVisible"
    preset="dialog"
    type="warning"
    :title="t(hasFilter ? 'sessions.fileTransfers.clearFilteredHistory' : 'sessions.fileTransfers.clearHistory')"
    :content="t('sessions.fileTransfers.clearPausedConfirm', { count: pendingHistorySelection?.pausedCount ?? 0 })"
    :positive-text="t('sessions.fileTransfers.confirmClear')"
    :negative-text="t('sessions.fileTransfers.cancel')"
    :positive-button-props="{ disabled: clearingHistory }"
    @positive-click="confirmHistoryClear"
    @negative-click="historyConfirmationVisible = false"
    @update:show="value => historyConfirmationVisible = value"
    @after-leave="() => { if (!historyConfirmationVisible) pendingHistorySelection = null; }"
  />
</template>
