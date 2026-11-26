<script setup lang="ts">
import { computed, watch } from "vue";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";
import { useTransferTaskActions } from "../composables/useTransferTaskActions.ts";
import TransferTaskDrawer from "./TransferTaskDrawer.vue";
import { formatTransferSessionLabel, type TransferBatchAction } from "../../../session/transfers/fileTransferTaskActions.ts";

const { closeGlobalTransferDrawer, globalTransferDrawerSessionId, globalTransferDrawerOpen, sessionTabs, sessionFileTransferManager, showAppMessage, t } = useVncAppContext();
const { runTaskPathAction, canOpenRemoteDirectory } = useTransferTaskActions();
const tasks = computed(() => sessionFileTransferManager.tasks.value.map(({ filePath: _filePath, ...task }) => task));
const sessionOptions = computed(() => sessionTabs.value.map((session) => ({ label: formatTransferSessionLabel(session), value: session.id })));
watch(sessionTabs, () => { if (globalTransferDrawerSessionId.value !== null && !sessionTabs.value.some((session) => session.id === globalTransferDrawerSessionId.value)) globalTransferDrawerSessionId.value = null; }, { deep: true });
function handlePathAction(action: Parameters<typeof runTaskPathAction>[2], sessionId: string, taskId: string) {
  void runTaskPathAction(sessionId, taskId, action);
}
function controlTask(action: "pause" | "resume" | "retry" | "cancel", taskId: string) {
  if (action === "pause") sessionFileTransferManager.pauseTask(taskId);
  if (action === "resume") sessionFileTransferManager.resumeTask(taskId);
  if (action === "retry") sessionFileTransferManager.retryTask(taskId);
  if (action === "cancel") sessionFileTransferManager.cancelTask(taskId);
}
function controlBatch(action: TransferBatchAction, taskIds: string[]) {
  const sessionId = globalTransferDrawerSessionId.value ?? undefined;
  if (action === "pause") sessionFileTransferManager.pauseAll(sessionId, taskIds);
  else sessionFileTransferManager.resumeAll(sessionId, taskIds);
}
function clearHistory(taskIds: string[]) {
  void sessionFileTransferManager.clearHistory(globalTransferDrawerSessionId.value ?? undefined, taskIds)
    .catch(() => showAppMessage(t("sessions.fileTransfers.clearHistoryFailed"), "error"));
}
</script>
<template>
  <TransferTaskDrawer :show="globalTransferDrawerOpen" :title="t('sessions.fileTransfers.globalDrawerTitle')" :tasks="tasks" :clearing-history="sessionFileTransferManager.clearingHistory.value" :can-open-remote-directory="canOpenRemoteDirectory" :empty-description="t('sessions.fileTransfers.globalEmpty')" :session-options="sessionOptions" :selected-session-id="globalTransferDrawerSessionId" show-session-filter @update:show="value => { if (!value) closeGlobalTransferDrawer(); }" @update:selected-session-id="value => { globalTransferDrawerSessionId = value; }" @task-action="controlTask" @batch-action="controlBatch" @path-action="handlePathAction" @clear-history="clearHistory" />
</template>
