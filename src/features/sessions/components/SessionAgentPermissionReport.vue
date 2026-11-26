<script setup lang="ts">
import { computed } from "vue";
import type { AgentPermissionDiagnosticsReport } from "../../../session/diagnostics/agentPermissionDiagnostics.ts";
import { useThemeVars } from "naive-ui";

const props = defineProps<{ report: AgentPermissionDiagnosticsReport }>();
const theme = useThemeVars();
const hasWritableCapability = computed(() => props.report.rows.some(row =>
  row.status === "granted" && (row.kind === "clipboard" || row.kind === "file_system")));
</script>

<template>
  <section class="session-permission-report" aria-label="远端能力与权限检查结果" aria-live="polite">
    <p class="session-diagnostic-note">{{ report.target }} · 检查于 {{ report.checkedAt }}</p>
    <ul>
      <li v-for="row in report.rows" :key="row.kind" :data-status="row.status">
        <div><strong>{{ row.label }}</strong><span class="session-permission-status">{{ row.statusText }}</span></div>
        <p v-if="row.actionHint" class="session-permission-hint">{{ row.actionHint }}</p>
      </li>
    </ul>
    <p v-if="hasWritableCapability" class="session-diagnostic-note">实际写入结果以发送或传输操作为准。</p>
  </section>
</template>

<style scoped>
.session-permission-report { min-width: 0; margin-top: 14px; }
ul { list-style: none; padding: 0; margin: 0; }
li { padding: 10px 0; border-top: 1px solid v-bind('theme.dividerColor'); }
li > div { display: flex; justify-content: space-between; align-items: baseline; gap: 8px; }
strong { font-size: 12px; color: v-bind('theme.textColor1'); }
.session-permission-status { flex-shrink: 0; font-size: 11px; color: v-bind('theme.textColor3'); }
[data-status="granted"] .session-permission-status { color: v-bind('theme.successColor'); }
[data-status="missing"] .session-permission-status { color: v-bind('theme.errorColor'); }
p { margin: 5px 0 0; line-height: 1.5; font-size: 12px; color: v-bind('theme.textColor3'); overflow-wrap: anywhere; }
.session-permission-hint { color: v-bind('theme.textColor2'); }
</style>
