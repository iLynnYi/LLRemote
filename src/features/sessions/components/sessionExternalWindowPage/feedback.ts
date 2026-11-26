import type { Ref } from "vue";
import type { SessionExternalWindowLocation } from "@session/external-window/sessionExternalWindow";
import type { SessionExternalWindowFeedbackPayload } from "@session/external-window/sessionExternalInput";

export function createExternalWindowFeedbackController(options: {
  externalWindowLocation: SessionExternalWindowLocation | null;
  windowFeedback: Ref<SessionExternalWindowFeedbackPayload | null>;
}) {
  let feedbackTimer: ReturnType<typeof window.setTimeout> | null = null;

  function showExternalWindowFeedback(
    message: string,
    tone: SessionExternalWindowFeedbackPayload["tone"],
  ) {
    if (!options.externalWindowLocation) {
      return;
    }
    options.windowFeedback.value = {
      sessionId: options.externalWindowLocation.sessionId,
      tone,
      message,
    };
    if (feedbackTimer) {
      window.clearTimeout(feedbackTimer);
    }
    feedbackTimer = window.setTimeout(() => {
      options.windowFeedback.value = null;
      feedbackTimer = null;
    }, 3200);
  }

  function showWindowFeedback(payload: SessionExternalWindowFeedbackPayload) {
    showExternalWindowFeedback(payload.message, payload.tone);
  }

  function clearFeedbackTimer() {
    if (feedbackTimer) {
      window.clearTimeout(feedbackTimer);
      feedbackTimer = null;
    }
  }

  return {
    clearFeedbackTimer,
    showExternalWindowFeedback,
    showWindowFeedback,
  };
}
