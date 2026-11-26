import type { Ref } from "vue";
import type { PasswordPromptState, Translate } from "@app/types";

export function createPasswordPromptController(options: {
  passwordPrompt: Ref<PasswordPromptState>;
  t: Translate;
  notify: (content: string, tone: "warning") => void;
}) {
  type Prompt = { resolve: (value: string | undefined) => void; releaseAbort: () => void; onUserCancel?: () => void };
  let pending: Prompt | null = null;

  function resetPasswordPrompt() {
    options.passwordPrompt.value = {
      visible: false,
      target: "",
      password: "",
    };
  }

  function finish(prompt: Prompt, value: string | undefined) {
    if (pending !== prompt) return;
    pending = null;
    prompt.releaseAbort();
    resetPasswordPrompt();
    prompt.resolve(value);
  }

  // Replacement and abort close a prompt silently; only an explicit dialog action calls onUserCancel.
  function requestSessionPassword(target: string, initialPassword = "", signal?: AbortSignal, onUserCancel?: () => void): Promise<string | undefined> {
    if (signal?.aborted) return Promise.resolve(undefined);
    if (pending) finish(pending, undefined);
    options.passwordPrompt.value = {
      visible: true,
      target,
      password: initialPassword,
    };

    return new Promise((resolve) => {
      const prompt: Prompt = { resolve, releaseAbort: () => undefined, onUserCancel };
      pending = prompt;
      if (signal) {
        const abort = () => finish(prompt, undefined);
        prompt.releaseAbort = () => signal.removeEventListener("abort", abort);
        signal.addEventListener("abort", abort, { once: true });
        if (signal.aborted) abort();
      }
    });
  }

  function cancelPasswordPrompt() {
    const prompt = pending;
    if (!prompt) return;
    finish(prompt, undefined);
    prompt.onUserCancel?.();
  }

  function submitPasswordPrompt(): boolean {
    if (!pending) return false;
    if (!options.passwordPrompt.value.password) {
      options.notify(options.t("sessions.passwordPromptEmpty"), "warning");
      return false;
    }

    const nextPassword = options.passwordPrompt.value.password;
    finish(pending, nextPassword);
    return true;
  }

  return {
    cancelPasswordPrompt,
    requestSessionPassword,
    submitPasswordPrompt,
  };
}
