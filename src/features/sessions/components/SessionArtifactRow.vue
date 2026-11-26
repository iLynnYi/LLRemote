<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { NButton, NDropdown, NIcon, NModal } from "naive-ui";
import { EllipsisHorizontalOutline, FolderOpenOutline } from "@vicons/ionicons5";
import type { SessionArtifact } from "../../../session/artifacts/sessionArtifacts.ts";
import { formatArtifactCreatedAt, formatArtifactDuration } from "../../../session/artifacts/sessionArtifactPresentation.ts";

const props = defineProps<{ artifact: SessionArtifact; nowMs: number }>();
const emit = defineEmits<{
  open: [artifact: SessionArtifact];
  reveal: [artifact: SessionArtifact];
  copyPath: [artifact: SessionArtifact];
}>();
const { t, locale } = useI18n();
const fileNameOpen = ref(false);
const time = computed(() => formatArtifactCreatedAt(props.artifact.createdAt, props.nowMs, locale.value));
const label = computed(() => [
  t(`sessions.artifacts.${props.artifact.kind}`),
  `${time.value.isToday ? t("sessions.artifacts.today") : time.value.date} ${time.value.time}`.trim(),
].join(" · "));
const metadata = computed(() => [
  props.artifact.kind === "recording" && props.artifact.durationSeconds !== undefined
    ? formatArtifactDuration(props.artifact.durationSeconds) : "",
  props.artifact.width && props.artifact.height ? `${props.artifact.width} × ${props.artifact.height}` : "",
].filter(Boolean).join(" · "));
const moreOptions = computed(() => [
  { key: "copy-path", label: t("sessions.artifacts.copyPath") },
  { key: "file-name", label: t("sessions.artifacts.viewFileName") },
]);
function selectMore(key: string) {
  if (key === "copy-path") emit("copyPath", props.artifact);
  if (key === "file-name") fileNameOpen.value = true;
}
</script>

<template>
  <article class="session-artifact-row">
    <button
      type="button"
      class="session-artifact-open"
      :title="`${artifact.fileName}\n${time.full}`"
      :aria-label="`${t('sessions.artifacts.openFile')}：${label}`"
      @click="emit('open', artifact)"
    >
      <strong>{{ label }}</strong>
      <small v-if="metadata">{{ metadata }}</small>
    </button>
    <div class="session-artifact-actions">
      <NButton quaternary circle size="tiny" :title="t('sessions.artifacts.revealFile')" :aria-label="t('sessions.artifacts.revealFile')" @click="emit('reveal', artifact)">
        <template #icon><NIcon><FolderOpenOutline /></NIcon></template>
      </NButton>
      <NDropdown trigger="click" placement="bottom-end" :options="moreOptions" @select="selectMore">
        <NButton quaternary circle size="tiny" :title="t('sessions.artifacts.more')" :aria-label="t('sessions.artifacts.more')">
          <template #icon><NIcon><EllipsisHorizontalOutline /></NIcon></template>
        </NButton>
      </NDropdown>
    </div>
    <NModal v-model:show="fileNameOpen" preset="dialog" :title="t('sessions.artifacts.fileName')" :positive-text="t('tabs.close')" @positive-click="fileNameOpen = false">
      <p class="session-artifact-file-name">{{ artifact.fileName }}</p>
    </NModal>
  </article>
</template>
