<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { NButton, NInput, NSelect, NTabPane, NTabs } from "naive-ui";
import type { SessionDiagnosticView } from "../../../session/diagnostics/sessionDiagnosticView.ts";
import type { EventLogEntry, EventLogLevel } from "../../../diagnostics/eventLog.ts";

const props = defineProps<{ view: SessionDiagnosticView }>();
const activeTab = ref("overview");
const keyword = ref("");
const level = ref<"all" | EventLogLevel>("all");
const visibleCount = ref(50);
const levelOptions = [
  { label: "全部级别", value: "all" },
  { label: "ERROR", value: "error" },
  { label: "WARN", value: "warning" },
  { label: "SUCCESS", value: "success" },
  { label: "INFO", value: "info" },
];
const filteredEvents = computed(() => props.view.recentEvents.filter((event) => {
  if (level.value !== "all" && event.level !== level.value) return false;
  const query = keyword.value.trim().toLocaleLowerCase();
  return !query || `${event.source} ${event.message}`.toLocaleLowerCase().includes(query);
}));
const visibleEvents = computed(() => filteredEvents.value.slice(0, visibleCount.value));
watch(() => [keyword.value, level.value, props.view.sessionId], () => { visibleCount.value = 50; });
watch(() => props.view.sessionId, () => { activeTab.value = "overview"; });

function eventLevel(event: EventLogEntry): string {
  return event.level === "warning" ? "WARN" : event.level.toUpperCase();
}
</script>

<template>
  <div class="session-diagnostic-view">
    <NTabs v-model:value="activeTab" type="line" animated>
      <NTabPane name="overview" tab="诊断概览">
        <div class="session-diagnostic-overview">
          <div class="session-diagnostic-view-heading">
            <div>
              <span class="session-diagnostic-eyebrow">当前会话</span>
              <strong>{{ view.target }}</strong>
            </div>
            <span class="session-diagnostic-state" :data-tone="view.tone">{{ view.stateText }} · {{ view.duration }}</span>
          </div>

          <div class="session-diagnostic-conclusion" :data-tone="view.issue ? (view.tone === 'failed' ? 'failed' : 'warning') : view.tone">
            <strong>{{ view.issue ? "需要关注" : "未记录到明确故障" }}</strong>
            <p v-if="view.issue">{{ view.issue }}</p>
          </div>

          <div class="session-diagnostic-view-grid">
            <div><span>基础连接</span><strong>{{ view.base }}</strong></div>
            <div><span>画面</span><strong>{{ view.displayEvidence }}</strong><small>{{ view.display }}</small></div>
            <div><span>操作</span><strong>{{ view.inputStatus }}</strong><small>{{ view.input }}</small></div>
            <div><span>增强功能</span><strong>{{ view.enhanced }}</strong></div>
          </div>

          <section v-if="view.userEvents.length" class="session-diagnostic-events">
            <h3>近期会话记录</h3>
            <ol>
              <li v-for="event in view.userEvents" :key="event.id" :data-tone="event.tone">
                <time>{{ event.time }}</time>
                <span>{{ event.message }}</span>
                <small v-if="event.count > 1">重复 {{ event.count }} 次</small>
              </li>
            </ol>
          </section>
        </div>
      </NTabPane>

      <NTabPane name="technical" tab="技术信息">
        <div class="session-diagnostic-technical">
          <div class="session-diagnostic-technical-heading">
            <div><strong>会话事件</strong></div>
            <span class="session-diagnostic-count">{{ filteredEvents.length }} / {{ view.recentEvents.length }} 条</span>
          </div>
          <div class="session-diagnostic-filters">
            <NInput v-model:value="keyword" clearable size="small" placeholder="搜索当前会话事件" aria-label="搜索当前会话事件" />
            <NSelect v-model:value="level" size="small" :options="levelOptions" aria-label="筛选日志级别" />
          </div>
          <p class="session-diagnostic-empty">仅含当前会话的内存事件，完整记录见应用日志。</p>
          <div v-if="filteredEvents.length" class="session-diagnostic-detail-list">
            <details v-for="event in visibleEvents" :key="event.id">
              <summary><span>{{ event.time }} · {{ eventLevel(event) }} · {{ event.source }}</span><span class="session-diagnostic-event-preview">{{ event.message }}</span></summary>
              <p>{{ event.message }}</p>
            </details>
            <NButton v-if="visibleCount < filteredEvents.length" secondary size="small" class="session-diagnostic-more" @click="visibleCount += 50">显示更多（剩余 {{ filteredEvents.length - visibleCount }} 条）</NButton>
          </div>
          <p v-else class="session-diagnostic-empty">没有符合条件的事件。</p>
          <details class="session-diagnostic-raw-report">
            <summary>技术报告原文</summary>
            <pre>{{ view.technicalReport }}</pre>
          </details>
        </div>
      </NTabPane>
    </NTabs>
  </div>
</template>

<style scoped>
.session-diagnostic-view { max-height: min(66vh, 720px); overflow: auto; padding: 0 4px 4px; }
.session-diagnostic-overview, .session-diagnostic-technical { display: grid; gap: 16px; padding: 15px 0 4px; }
.session-diagnostic-view-heading { display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; }
.session-diagnostic-view-heading > div { display: grid; gap: 5px; min-width: 0; }
.session-diagnostic-view-heading strong { overflow-wrap: anywhere; font-size: 17px; }
.session-diagnostic-eyebrow, .session-diagnostic-view-grid span { color: #687587; font-size: 12px; }
.session-diagnostic-state { flex: none; border-radius: 999px; padding: 6px 10px; background: #edf7f3; color: #14735e; font-size: 12px; font-weight: 700; }
.session-diagnostic-state[data-tone="failed"] { background: #fff0ef; color: #a33a35; }
.session-diagnostic-state[data-tone="working"] { background: #fff5e7; color: #9a641f; }
.session-diagnostic-conclusion { border-radius: 8px; border-left: 3px solid #53b997; background: #f3faf7; padding: 12px 14px; }
.session-diagnostic-conclusion[data-tone="failed"] { border-left-color: #cf514a; background: #fff5f4; }
.session-diagnostic-conclusion[data-tone="warning"] { border-left-color: #c68a2d; background: #fff8eb; }
.session-diagnostic-conclusion p { margin: 5px 0 0; line-height: 1.55; }
.session-diagnostic-view-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }
.session-diagnostic-view-grid > div { display: grid; gap: 4px; min-width: 0; padding: 10px 12px; border: 1px solid #e5e9ee; border-radius: 8px; }
.session-diagnostic-view-grid strong { overflow-wrap: anywhere; font-size: 13px; }
.session-diagnostic-view-grid small { color: #687587; font-size: 11px; }
.session-diagnostic-events h3 { margin: 0 0 9px; font-size: 14px; }
.session-diagnostic-events ol { display: grid; gap: 7px; list-style: none; margin: 0; padding: 0; }
.session-diagnostic-events li { display: flex; gap: 12px; align-items: baseline; overflow-wrap: anywhere; padding: 8px 10px; border-radius: 6px; background: #f6f8fa; font-size: 12px; }
.session-diagnostic-events li[data-tone="error"] { background: #fff4f3; }
.session-diagnostic-events li[data-tone="warning"] { background: #fff8eb; }
.session-diagnostic-events time { flex: none; color: #667586; font-size: 11px; font-weight: 700; }
.session-diagnostic-events li small { margin-left: auto; flex: none; color: #667586; }
.session-diagnostic-empty { margin: 0; color: #687587; font-size: 12px; }
.session-diagnostic-technical-heading { display: flex; justify-content: space-between; align-items: start; gap: 12px; }
.session-diagnostic-technical-heading > div { display: grid; gap: 4px; min-width: 0; }
.session-diagnostic-technical-heading strong { font-size: 14px; }
.session-diagnostic-technical-heading span { color: #687587; font-size: 12px; }
.session-diagnostic-count { flex: none; padding: 4px 8px; border-radius: 999px; background: #f0f5f3; color: #22745e !important; font-weight: 700; }
.session-diagnostic-filters { display: grid; grid-template-columns: minmax(0, 1fr) 120px; gap: 8px; }
.session-diagnostic-detail-list { display: grid; gap: 7px; }
.session-diagnostic-detail-list details { min-width: 0; border-radius: 6px; background: #f6f8fa; padding: 8px 10px; font-size: 12px; }
.session-diagnostic-detail-list summary { display: flex; min-width: 0; gap: 12px; cursor: pointer; }
.session-diagnostic-detail-list summary > span:first-child { flex: none; max-width: 55%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: #667586; font-weight: 700; }
.session-diagnostic-event-preview { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.session-diagnostic-detail-list details p { margin: 9px 0 0; overflow-wrap: anywhere; white-space: pre-wrap; line-height: 1.5; }
.session-diagnostic-more { justify-self: center; }
.session-diagnostic-raw-report summary { cursor: pointer; font-weight: 700; }
.session-diagnostic-raw-report pre { max-height: 280px; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; font-size: 11px; }
@media (max-width: 600px) { .session-diagnostic-view-heading { display: grid; } .session-diagnostic-view-grid { grid-template-columns: 1fr; } .session-diagnostic-technical-heading { flex-wrap: wrap; } .session-diagnostic-detail-list summary { display: grid; gap: 3px; } .session-diagnostic-detail-list summary > span:first-child { max-width: 100%; } }
</style>
