<script setup lang="ts">
import { CopyOutline } from "@vicons/ionicons5";
import { NButton, NEmpty, NIcon } from "naive-ui";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";
import { showDeveloperDebugInfo as resolveShowDeveloperDebugInfo } from "../../../app/env.ts";

import { useSessionAdvancedDiagnostics } from "../composables/useSessionAdvancedDiagnostics.ts";
import SessionAgentPermissionReport from "./SessionAgentPermissionReport.vue";
import SessionDeveloperDiagnostics from "./SessionDeveloperDiagnostics.vue";
import SessionRecordingJobs from "./SessionRecordingJobs.vue";
import SessionArtifactRow from "./SessionArtifactRow.vue";
import { artifactDirectoryName } from "../../../session/artifacts/sessionArtifactPresentation.ts";

const {
  copyDiagnosticReport,
  activeSessionArtifacts,
  recordingJobs,
  retryRecordingJob,
  copySessionArtifactPath,
  openSessionArtifactFile,
  revealSessionArtifactFile,
  sessionDiagnosticsSummary,
  sessionDuration,
  nowTick,
  sessionArtifactDirectoryDisplayPath,
  showAppMessage,
  stabilityStatus,
  t,
} = useVncAppContext();

const { canCheckPermissions, checkingPermissions, checkPermissions, permissionReport, permissionError, developerDiagnosticsExpanded } = useSessionAdvancedDiagnostics();

const showDeveloperDebugInfo = resolveShowDeveloperDebugInfo();

function displayPlacementText(): string {
  return sessionDiagnosticsSummary.value.externalWindowOwned
    ? t("diagnostics.externalWindowOwned")
    : t(`diagnostics.renderStates.${sessionDiagnosticsSummary.value.renderState}`);
}

function latestInputText(): string {
  const latestInput = sessionDiagnosticsSummary.value.latestInput;
  return latestInput.source === "none"
    ? "-"
    : `${t(`diagnostics.inputSources.${latestInput.source}`)} · ${t(`diagnostics.inputTypes.${latestInput.type}`)} · ${latestInput.atText}`;
}

function latestInputTitle(): string {
  const latestInput = sessionDiagnosticsSummary.value.latestInput;
  return latestInput.source === "none" ? "-" : latestInput.at;
}

async function copySelectedTarget() {
  const target = sessionDiagnosticsSummary.value.selectedTarget;
  if (!target) {
    return;
  }

  try {
    await navigator.clipboard.writeText(target);
    showAppMessage(t("diagnostics.targetCopied"), "success");
  } catch (error) {
    showAppMessage(t("diagnostics.targetCopyFailed", { error: String(error) }), "error");
  }
}
</script>

<template>
  <aside class="session-diagnostics">
    <section class="session-detail-card" :data-tone="sessionDiagnosticsSummary.tone">
      <header class="session-detail-header">
        <div class="session-detail-title-row">
          <span>{{ t("diagnostics.sessionDetails") }}</span>
        </div>
        <div class="session-detail-target-row">
          <strong :title="sessionDiagnosticsSummary.selectedTarget || '-'">{{ sessionDiagnosticsSummary.selectedTarget || "-" }}</strong>
          <NButton
            quaternary
            circle
            size="tiny"
            :disabled="!sessionDiagnosticsSummary.selectedTarget"
            :title="t('diagnostics.copyTarget')"
            :aria-label="t('diagnostics.copyTarget')"
            @click="copySelectedTarget"
          >
            <template #icon>
              <NIcon><CopyOutline /></NIcon>
            </template>
          </NButton>
        </div>
        <div class="session-detail-status-row">
          <span class="session-status-badge" :data-state="sessionDiagnosticsSummary.state" :data-tone="sessionDiagnosticsSummary.tone">
            {{ t(`connection.${sessionDiagnosticsSummary.state}`) ?? sessionDiagnosticsSummary.state }}
          </span>
          <span class="session-duration-badge">{{ sessionDuration }}</span>
        </div>
      </header>

      <div class="session-detail-basic-grid">
        <div>
          <span>{{ t("diagnostics.inputMode") }}</span>
          <strong>{{ t(`sessions.inputModes.${sessionDiagnosticsSummary.inputMode}`) }}</strong>
        </div>
        <div>
          <span>{{ t("diagnostics.displayPlacement") }}</span>
          <strong>{{ displayPlacementText() }}</strong>
        </div>
        <div>
          <span>画面来源</span>
          <strong>{{ sessionDiagnosticsSummary.transportLayerTexts.displaySource }}</strong>
        </div>
        <div>
          <span>输入方式</span>
          <strong>{{ sessionDiagnosticsSummary.transportLayerTexts.inputPath }}</strong>
        </div>
      </div>

      <details class="session-advanced-diagnostics">
        <summary>
          <span>高级诊断</span>
          <strong>{{ sessionDiagnosticsSummary.transportLayerTexts.issueLayer }}</strong>
        </summary>
        <div class="session-advanced-grid">
          <div>
            <span>{{ t("status.stability") }}</span>
            <strong>{{ t(`diagnostics.stabilityStates.${stabilityStatus}`) }}</strong>
          </div>
          <div>
            <span>基础连接</span>
            <strong>{{ sessionDiagnosticsSummary.transportLayerTexts.base }}</strong>
          </div>
          <div>
            <span>增强能力</span>
            <strong>{{ sessionDiagnosticsSummary.transportLayerTexts.enhanced }}</strong>
          </div>
          <div>
            <span>问题归属</span>
            <strong>{{ sessionDiagnosticsSummary.transportLayerTexts.issueLayer }}</strong>
          </div>
          <div class="session-detail-wide">
            <span>{{ t("diagnostics.latestInput") }}</span>
            <strong :title="latestInputTitle()">{{ latestInputText() }}</strong>
          </div>
          <div v-if="sessionDiagnosticsSummary.appServer" class="session-detail-wide">
            <span>连接识别</span>
            <strong>{{ sessionDiagnosticsSummary.appServer.evidenceText }}</strong>
          </div>
          <div v-if="sessionDiagnosticsSummary.appServer" class="session-detail-wide">
            <span>增强服务 / 输入</span>
            <strong>{{ sessionDiagnosticsSummary.appServer.agentText }} · {{ sessionDiagnosticsSummary.appServer.inputStatusText }}</strong>
          </div>
          <div v-if="sessionDiagnosticsSummary.fallbackError" class="session-detail-wide">
            <span>回退原因</span>
            <strong :title="sessionDiagnosticsSummary.fallbackError">{{ sessionDiagnosticsSummary.fallbackError }}</strong>
          </div>
        </div>
        <div class="session-diagnostic-actions">
          <NButton secondary size="small" @click="copyDiagnosticReport()">{{ t('sessions.toolbox.actions.copy-diagnostic-summary.label') }}</NButton>
          <NButton v-if="canCheckPermissions" secondary size="small" :loading="checkingPermissions" @click="checkPermissions">{{ t('sessions.toolbox.actions.check-agent-permissions.label') }}</NButton>
        </div>
        <p v-if="permissionError" class="session-permission-error" role="alert">{{ permissionError }}</p>
        <SessionAgentPermissionReport v-if="permissionReport" :report="permissionReport" />
      </details>
    </section>

    <SessionDeveloperDiagnostics
      v-if="showDeveloperDebugInfo"
      v-model:expanded="developerDiagnosticsExpanded"
      :summary="sessionDiagnosticsSummary"
    />

    <section class="session-artifacts">
      <div class="diagnostic-card-heading">
        <span>{{ t("sessions.artifacts.title") }}</span>
      </div>
      <p class="session-artifact-message" :title="sessionArtifactDirectoryDisplayPath || undefined">
        {{ sessionArtifactDirectoryDisplayPath
          ? t("sessions.artifacts.saveLocation", { directory: artifactDirectoryName(sessionArtifactDirectoryDisplayPath) })
          : t("sessions.artifacts.directoryFromSettings") }}
      </p>
      <SessionRecordingJobs :jobs="recordingJobs" @retry="retryRecordingJob" @open="openSessionArtifactFile" @reveal="revealSessionArtifactFile" />
      <div v-if="activeSessionArtifacts.length" class="session-artifact-list" aria-live="polite" aria-relevant="additions">
        <SessionArtifactRow
          v-for="artifact in activeSessionArtifacts"
          :key="artifact.id"
          :artifact="artifact"
          :now-ms="nowTick"
          @open="openSessionArtifactFile"
          @reveal="revealSessionArtifactFile"
          @copy-path="copySessionArtifactPath"
        />
      </div>
      <NEmpty
        v-else-if="!recordingJobs.length"
        size="small"
        class="session-artifact-empty"
        :description="t('sessions.artifacts.empty')"
      />
    </section>
  </aside>
</template>
