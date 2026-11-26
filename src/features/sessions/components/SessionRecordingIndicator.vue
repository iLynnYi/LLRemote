<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { RecordingSpaceStatus } from "../../../session/artifacts/sessionRecordingTask.ts";
import { formatArtifactDuration } from "../../../session/artifacts/sessionArtifactPresentation.ts";

const props = defineProps<{ elapsedSeconds: number; space?: RecordingSpaceStatus; stopping?: boolean }>();
const emit = defineEmits<{ stop: [] }>();
const { t } = useI18n();
const warning = computed(() => Boolean(props.space?.mergeWarning));
const elapsed = computed(() => formatArtifactDuration(props.elapsedSeconds));
const title = computed(() => {
  if (!warning.value || !props.space) return t("sessions.artifacts.stopRecording");
  const format = (bytes: number) => `${(bytes / 1024 ** 3).toFixed(2)} GiB`;
  return t("sessions.artifacts.recordingWarningTooltip", { available: format(props.space.availableBytes), required: format(props.space.mergeRequiredBytes) });
});
</script>

<template>
  <button type="button" class="session-recording-indicator" :class="{ 'is-warning': warning }"
    :disabled="stopping" :title="title" :aria-label="`${title} · ${elapsed}`"
    @pointerdown.stop @click.stop="emit('stop')">
    <span aria-hidden="true">{{ warning ? "⚠" : "●" }}</span> {{ elapsed }}
  </button>
</template>

<style scoped>
.session-recording-indicator {
  position: absolute;
  right: calc(100% + 8px);
  top: 50%;
  transform: translateY(-50%);
  padding: 4px 8px;
  border: 0;
  border-radius: 5px;
  color: #fff;
  background: #b4232d;
  font: inherit;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  cursor: pointer;
}

.session-recording-indicator:hover {
  background: #921d27;
}

.session-recording-indicator.is-warning { background: #f0b429; color: #322300; }
.session-recording-indicator.is-warning:hover { background: #d99a13; }

.session-recording-indicator:focus-visible {
  outline: 2px solid #fff;
  outline-offset: 2px;
}

.session-recording-indicator:disabled {
  cursor: wait;
  opacity: 0.75;
}
</style>
