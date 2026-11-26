<script setup lang="ts">
import { NButton, NProgress } from "naive-ui";
import { useI18n } from "vue-i18n";
import type { RecordingJob } from "../composables/useSessionArtifacts.ts";
import type { SessionArtifact } from "../../../session/artifacts/sessionArtifacts.ts";
import { formatArtifactDuration } from "../../../session/artifacts/sessionArtifactPresentation.ts";

defineProps<{ jobs: RecordingJob[] }>();
const emit = defineEmits<{
  retry: [id: string, chooseDirectory?: boolean]; open: [artifact: SessionArtifact]; reveal: [artifact: SessionArtifact];
}>();
const { t } = useI18n();
const busy = (job: RecordingJob) => ["saving", "merging", "verifying"].includes(job.status);
function artifact(job: RecordingJob, file?: { filePath: string; fileName: string }): SessionArtifact {
  return { id: job.recordingId, recordingId: job.recordingId, sessionId: "", target: "", createdAt: "",
    kind: "recording", mimeType: "video/webm", filePath: file?.filePath ?? job.recoveryPath, fileName: file?.fileName ?? job.fileName };
}
</script>

<template>
  <section v-if="jobs.length" class="recording-jobs" :aria-label="t('sessions.artifacts.recordingJobs')">
    <strong>{{ t('sessions.artifacts.recordingJobs') }}</strong>
    <article v-for="job in jobs" :key="job.recordingId" class="recording-job" :aria-busy="busy(job)">
      <div class="recording-job-heading">
        <span :title="job.fileName">{{ job.fileName }}</span>
        <small>{{ formatArtifactDuration(job.durationMs / 1000) }}</small>
      </div>
      <p role="status">{{ t(`sessions.artifacts.recordingJobStates.${job.status}`) }}</p>
      <NProgress v-if="busy(job)" type="line" :percentage="job.progress" :show-indicator="job.status === 'merging'" :processing="true" />
      <details v-if="job.files.length">
        <summary>{{ t('sessions.artifacts.recordingFiles', { count: job.files.length }) }}</summary>
        <div v-for="file in job.files" :key="file.filePath" class="recording-job-file">
          <button type="button" :title="file.fileName" @click="emit('open', artifact(job, file))">{{ file.fileName }}</button>
          <NButton quaternary size="tiny" @click="emit('reveal', artifact(job, file))">{{ t('sessions.artifacts.revealFile') }}</NButton>
        </div>
      </details>
      <div v-if="!busy(job) && job.status !== 'saved'" class="recording-job-actions">
        <NButton secondary size="small" @click="emit('retry', job.recordingId)">{{ t('sessions.artifacts.recordingRetryMerge') }}</NButton>
        <NButton secondary size="small" @click="emit('retry', job.recordingId, true)">{{ t('sessions.artifacts.recordingSaveElsewhere') }}</NButton>
        <NButton quaternary size="small" @click="emit('reveal', artifact(job))">{{ t('sessions.artifacts.recordingRevealRecovery') }}</NButton>
      </div>
    </article>
  </section>
</template>

<style scoped>
.recording-jobs { display: grid; gap: 10px; margin-top: 12px; }
.recording-job { min-width: 0; padding: 10px; border: 1px solid var(--n-border-color, #8885); border-radius: 6px; }
.recording-job-heading { display: flex; gap: 8px; align-items: center; }
.recording-job-heading > span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.recording-job p { margin: 6px 0; font-size: 12px; }
.recording-job-actions { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 8px; }
.recording-job-file { display: flex; align-items: center; gap: 6px; }
.recording-job-file > button { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; background: transparent; border: none; color: inherit; cursor: pointer; text-align: left; padding: 6px 0; }
.recording-job-file > button:focus-visible { outline: 2px solid currentColor; outline-offset: 2px; }
summary { cursor: pointer; margin-top: 8px; }
</style>
