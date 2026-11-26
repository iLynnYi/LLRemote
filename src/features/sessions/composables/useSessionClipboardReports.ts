import type { ComputedRef, Ref } from "vue";
import type { Translate } from "@app/types";
import type { EventLogLevel } from "@diagnostics/eventLog";

type SessionClipboardReportsOptions = {
  diagnosticReport: ComputedRef<string>;
  errorMessage: Ref<string>;
  logEvent: (level: EventLogLevel, source: string, message: string) => void;
  notify: (content: string, tone: "success" | "error") => void;
  t: Translate;
};

export function useSessionClipboardReports(options: SessionClipboardReportsOptions) {
  const {
    diagnosticReport,
    errorMessage,
    logEvent,
    t,
  } = options;

  async function copyDiagnosticReport(notify = options.notify) {
    try {
      await navigator.clipboard.writeText(diagnosticReport.value);
      logEvent("success", t("log.sourceSession"), t("log.diagnosticCopied"));
      notify(t("log.diagnosticCopied"), "success");
    } catch (error) {
      errorMessage.value = String(error);
      logEvent("error", t("log.sourceSession"), t("log.diagnosticCopyFailed", { error: String(error) }));
      notify(t("log.diagnosticCopyFailed", { error: String(error) }), "error");
    }
  }

  return {
    copyDiagnosticReport,
  };
}
