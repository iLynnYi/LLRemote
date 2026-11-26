<script setup lang="ts">
import { computed } from "vue";
import type { SessionDiagnosticsPanelSummary } from "../../../session/diagnostics/sessionDiagnosticsPanel.ts";

const props = defineProps<{ summary: SessionDiagnosticsPanelSummary; expanded: boolean }>();
const emit = defineEmits<{ "update:expanded": [expanded: boolean] }>();
const clipboardRows = computed(() => [
  { label: "本机 → 远端", activity: props.summary.clipboard.clientToServer },
  { label: "远端 → 本机", activity: props.summary.clipboard.serverToClient },
]);

function updateExpanded(event: Event) {
  if (event.target instanceof HTMLDetailsElement) emit("update:expanded", event.target.open);
}
</script>

<template>
  <details class="session-debug-card" :open="expanded" @toggle="updateExpanded">
    <summary>开发者诊断</summary>
    <p class="session-diagnostic-note">以下是最近一次画面提交的证据，不能代表当前连接健康状态。平均提交率包含静止时间，不代表流畅度。</p>
    <div class="session-debug-raw-grid">
      <div><span>采样证据</span><strong>{{ summary.runtimeEvidence.healthText }}</strong></div>
      <div><span>采样时间</span><strong>{{ summary.runtimeEvidence.capturedAt }}</strong></div>
      <div><span>Canvas</span><strong>{{ summary.runtimeEvidence.canvasCount }} · {{ summary.runtimeEvidence.canvasSize }}</strong></div>
      <div><span>Framebuffer</span><strong>{{ summary.runtimeEvidence.framebuffer }}</strong></div>
      <div class="session-detail-wide"><span>传输标识</span><strong>{{ summary.runtimeEvidence.transportId }}</strong></div>
      <div class="session-detail-wide"><span>Surface 提交统计</span><strong>{{ summary.runtimeEvidence.surface }}</strong></div>
      <div v-if="summary.appServer?.capabilityBoundary === 'local_server_reported'" class="session-detail-wide">
        <span>本机 Server 采集与发送</span><strong>{{ summary.appServer.frameDiagnostics ?? '尚未上报' }}</strong>
      </div>
      <div class="session-detail-wide"><span>最近提交事件</span><strong>{{ summary.runtimeEvidence.latestCoreEvent }}</strong></div>
      <div class="session-detail-wide">
        <span>剪贴板策略</span>
        <strong>mode={{ summary.clipboard.mode }} · enabled={{ summary.clipboard.enabled }} · readonly={{ summary.clipboard.readonly }}</strong>
      </div>
      <div v-for="row in clipboardRows" :key="row.label" class="session-detail-wide">
        <span>{{ row.label }}</span>
        <strong v-if="row.activity">{{ row.activity.atText }} · {{ row.activity.statusText }} · {{ row.activity.channelText }} · {{ row.activity.bytes }} 字节 · {{ row.activity.reason }}</strong>
        <strong v-else>尚无同步记录</strong>
      </div>
      <div class="session-detail-wide">
        <span>增强通道 / 能力</span><strong>{{ summary.remoteSessionCore?.enhancedChannel ?? summary.transportLayers.enhanced }}</strong>
      </div>
      <template v-if="summary.remoteSessionCore">
        <div class="session-detail-wide"><span>Core 会话</span><strong>{{ summary.remoteSessionCore.sessionId }} · {{ summary.remoteSessionCore.state }}</strong></div>
        <div class="session-detail-wide">
          <span>Core Surface 归属</span>
          <strong>{{ summary.remoteSessionCore.activeSurfaceOwner }} · {{ summary.remoteSessionCore.activeSurfaceWindow }} · {{ summary.remoteSessionCore.surfaceCount }} 个</strong>
        </div>
      </template>
    </div>
  </details>
</template>

<style scoped>
.session-debug-card { display: block; }
summary { cursor: pointer; font-size: 13px; font-weight: 700; }
.session-debug-raw-grid strong { white-space: normal; overflow-wrap: anywhere; font-size: 12px; font-weight: 500; }
</style>
