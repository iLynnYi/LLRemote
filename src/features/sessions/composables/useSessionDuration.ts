import { computed, ref, watch, type Ref } from "vue";
import {
  elapsedRemoteSessionSeconds,
  formatDuration,
  formatStabilityCheckpointEvent,
  shouldRecordStabilityCheckpoint,
  stabilityLabel,
} from "@session/diagnostics/sessionDiagnostics";
import type { RemoteSession } from "@session/core/sessionManager";

export type SessionDurationOptions = {
  activeRemoteSession: Ref<RemoteSession | null>;
  logStabilityCheckpoint: (message: string) => void;
};

export function useSessionDuration({
  activeRemoteSession,
  logStabilityCheckpoint,
}: SessionDurationOptions) {
  const nowTick = ref(Date.now());
  const stabilityCheckpointRecorded = ref(false);
  let durationTimer: ReturnType<typeof setInterval> | null = null;

  const sessionSeconds = computed(() => elapsedRemoteSessionSeconds(activeRemoteSession.value, nowTick.value));
  const sessionDuration = computed(() => formatDuration(sessionSeconds.value));
  const stabilityStatus = computed(() => stabilityLabel(sessionSeconds.value));

  function startDurationTimer() {
    nowTick.value = Date.now();
    stabilityCheckpointRecorded.value = false;

    if (durationTimer) {
      clearInterval(durationTimer);
    }

    durationTimer = setInterval(() => {
      nowTick.value = Date.now();
    }, 1000);
  }

  function stopDurationTimer() {
    if (durationTimer) {
      clearInterval(durationTimer);
      durationTimer = null;
    }
  }

  watch(sessionSeconds, (currentSeconds, previousSeconds) => {
    if (!activeRemoteSession.value?.connectedAt) {
      return;
    }

    if (!shouldRecordStabilityCheckpoint({
      previousSeconds,
      currentSeconds,
      alreadyRecorded: stabilityCheckpointRecorded.value,
    })) {
      return;
    }

    stabilityCheckpointRecorded.value = true;
    logStabilityCheckpoint(formatStabilityCheckpointEvent({
      target: activeRemoteSession.value.target,
      duration: sessionDuration.value,
    }));
  });

  return {
    durationTimer,
    nowTick,
    sessionDuration,
    sessionSeconds,
    stabilityCheckpointRecorded,
    stabilityStatus,
    startDurationTimer,
    stopDurationTimer,
  };
}
