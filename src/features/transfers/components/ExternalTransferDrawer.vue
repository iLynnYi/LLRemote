<script setup lang="ts">
import type { TransferBatchAction, TransferTaskPathAction } from "../../../session/transfers/fileTransferTaskActions.ts";
import TransferTaskDrawer, { type TransferTaskView } from "./TransferTaskDrawer.vue";
defineProps<{ show: boolean; tasks: TransferTaskView[]; title: string; emptyDescription: string; canOpenRemoteDirectory: boolean; clearingHistory: boolean }>();
const emit = defineEmits<{ "update:show": [value: boolean]; "task-action": [action: "pause" | "resume" | "retry" | "cancel", taskId: string]; "batch-action": [action: TransferBatchAction, taskIds: string[]]; "clear-history": [taskIds: string[]]; "path-action": [action: TransferTaskPathAction, sessionId: string, taskId: string] }>();
function handleTaskAction(action: "pause" | "resume" | "retry" | "cancel", taskId: string) { emit("task-action", action, taskId); }
function handleBatchAction(action: TransferBatchAction, taskIds: string[]) { emit("batch-action", action, taskIds); }
</script>
<template>
  <TransferTaskDrawer :show="show" :title="title" :tasks="tasks" :clearing-history="clearingHistory" :can-open-remote-directory="() => canOpenRemoteDirectory" :empty-description="emptyDescription" @update:show="emit('update:show', $event)" @task-action="handleTaskAction" @batch-action="handleBatchAction" @path-action="(action, sessionId, taskId) => emit('path-action', action, sessionId, taskId)" @clear-history="emit('clear-history', $event)" />
</template>
