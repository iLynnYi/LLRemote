<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { AddOutline, CogOutline, CreateOutline, FolderOpenOutline, InformationCircleOutline, KeypadOutline, OpenOutline, ShareSocialOutline, TrashOutline } from "@vicons/ionicons5";
import { NButton, NEmpty, NIcon, NRadio, NRadioGroup, NSelect, NSwitch, useMessage } from "naive-ui";
import { isTauri } from "@tauri-apps/api/core";
import type { PostConnectPresentation } from "../../../session/core/connectionConfirm.ts";
import { SESSION_LOCAL_SHORTCUTS } from "../../../session/input/sessionLocalShortcuts.ts";
import { detectKeyboardShortcutDisplayPlatform, formatKeyboardShortcutForPlatform, formatKeyboardShortcutSymbolsForPlatform } from "../../../session/input/keyboardShortcut.ts";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";
import { fallbackAppMetadata, loadAppMetadata } from "../../../app/appMetadata.ts";
import appIcon from "../../../../src-tauri/icons/icon-source.svg";

import WindowsServiceSettings from "./WindowsServiceSettings.vue";
import MacSharingSettings from "./MacSharingSettings.vue";
import LocalShareSecuritySettings from "./LocalShareSecuritySettings.vue";
import DistributionInformation from "./DistributionInformation.vue";
import type { WindowsServiceAction } from "../../../server/windowsService.ts";
import type { AppMessageTone } from "../../../app/types.ts";
import { windowsServiceErrorKey } from "../../../server/windowsService.ts";
import { backgroundShareErrorKey } from "../../../server/backgroundShare.ts";
import { getLocalVncServerStartErrorCopy } from "../../../server/localShare.ts";
import { useBackgroundPermissionRepairFeedback, useSharingStartupSettingsFeedback } from "../../local-share/composables/useLocalShareFeedback.ts";

const app = useVncAppContext();
const {
  activeWorkspace,
  windowsService, manageSharingService, openWindowsServiceSetup, backgroundShareOverview, localVncServerLoading,
  sharingMode, sharingAutoStart, sharingAutoStartKnown, sharingAutoStartBusy, setSharingAutoStart, refreshSharingSettings,
  selectLocalShareWorkspace,
  requestedSettingsCategory, refreshLocalVncServerStatus,
  requestedSharingRepair, backgroundRestartSupported, backgroundRuntimeOutdated, backgroundPermissionRepairNeeded,
  restartBackgroundShare, updateAndRepairBackgroundShare, localShareStartPending, cancelLocalShareStart,
  localSharePermissionCheckError, sharingRuntimeError,
  artifactDirectoryOptions,
  canRevealSessionArtifactDirectory,
  chooseFileAccessDirectory,
  chooseBuiltinFileAccessDirectory,
  chooseSessionArtifactDirectory,
  fileAccessDirectories,
  fileAccessDirectoryChoosing,
  fileAccessMessage,
  fileAccessMessageTone,
  revealFileAccessDirectory,
  sessionSettings,
  revealSessionArtifactDirectory,
  revokeFileAccessDirectory,
  setConfirmBeforeConnect,
  setConnectionDefault,
  setPostConnectPresentation,
  setSessionArtifactDirectory,
  setFileTransferRenamePolicy,
  t,
} = app;

const postConnectPresentationOptions: PostConnectPresentation[] = ["main", "fullscreen", "external"];
const fileTransferRenameOptions = computed(() => ["autoRename", "overwrite", "skip"].map(value => ({
  value, label: t(`settings.fileTransferPolicies.${value}`),
})));
const appMetadata = ref(fallbackAppMetadata());
const showBuildRuntimeInfo = import.meta.env.DEV;
const appBuildLabel = import.meta.env.VITE_BUILD_LABEL ?? import.meta.env.MODE;
const platformLabel = computed(() => {
  const navigatorLike = globalThis.navigator;
  const parts = [navigatorLike?.platform, navigatorLike?.userAgent]
    .filter((part): part is string => Boolean(part));
  return parts.length ? parts.join(" / ") : t("settings.about.unknown");
});
const runModeLabel = computed(() =>
  isTauri() ? t("settings.about.desktopMode") : t("settings.about.previewMode"),
);
const startupRegistrationHint = computed(() => {
  if (sharingMode.value !== "background" || !backgroundShareOverview.value) return "";
  const { autoStart, startupRegistration } = backgroundShareOverview.value;
  if (!autoStart) return "";
  return t(`localShare.background.registration.${startupRegistration ?? "unknown"}`);
});
const startupRegistrationNeedsAction = computed(() => Boolean(startupRegistrationHint.value)
  && !(backgroundShareOverview.value?.autoStart && backgroundShareOverview.value.startupRegistration === "enabled"));
const startupEnabled = computed(() => sharingMode.value === "windowsService"
  ? windowsService.overview.value?.autoStart === true
  : sharingMode.value === "background" && backgroundShareOverview.value?.autoStart === true);
const startupAttemptFailed = computed(() => startupEnabled.value && (sharingMode.value === "windowsService"
  ? windowsService.overview.value?.lastServiceAttempt?.state
  : backgroundShareOverview.value?.lastLoginAttempt?.state) === "failed");
const startupAttemptText = computed(() => {
  if (!sharingAutoStartKnown.value) return t("localShare.runtimeChecking");
  const service = sharingMode.value === "windowsService";
  const attempt = service ? windowsService.overview.value?.lastServiceAttempt : backgroundShareOverview.value?.lastLoginAttempt;
  if (!attempt) return t(service ? "localShare.windowsService.noAttempt" : "localShare.background.noAttempt");
  const date = new Date(attempt.startedAtMs);
  const time = Number.isNaN(date.getTime()) ? "—" : date.toLocaleString("zh-CN");
  let result = t(`localShare.background.attemptState.${attempt.state}`);
  if (attempt.state === "failed" && attempt.errorCode) {
    const code = attempt.errorCode;
    const copy = getLocalVncServerStartErrorCopy(code);
    const detail = code.startsWith("background_share_") ? t(backgroundShareErrorKey(code))
      : code.startsWith("windows_service_") ? t(windowsServiceErrorKey(code))
        : copy ? t(copy.messageKey) : code;
    result += `：${detail}`;
  }
  return t(service ? "localShare.windowsService.lastAttempt" : "localShare.background.lastAttempt", { time, result });
});
const tauriBackendStatus = computed(() =>
  isTauri() ? t("settings.about.tauriBackendAvailable") : t("settings.about.tauriBackendPreview"),
);

onMounted(async () => {
  appMetadata.value = await loadAppMetadata();
});

const shortcutDisplayPlatform = detectKeyboardShortcutDisplayPlatform();

const sessionShortcutRows = computed(() => SESSION_LOCAL_SHORTCUTS
  .filter((shortcut) => shortcut.scope === "session-viewport")
  .map((shortcut) => ({
    actionId: shortcut.actionId,
    nameKey: shortcut.nameKey,
    label: formatKeyboardShortcutSymbolsForPlatform(shortcut.shortcut, shortcutDisplayPlatform),
    fullLabel: formatKeyboardShortcutForPlatform(shortcut.shortcut, shortcutDisplayPlatform),
  })),
);

const keyboardTransmissionRows = computed(() => shortcutDisplayPlatform === "macos"
  ? [
    { localLabel: "Command", localSymbol: "⌘", remoteLabel: "Win", remoteSymbol: "windows" },
    { localLabel: "Option", localSymbol: "⌥", remoteLabel: "Alt" },
    { localLabel: "Control", localSymbol: "⌃", remoteLabel: "Ctrl" },
    { localLabel: "Delete", remoteLabel: "Backspace" },
    { localLabel: "Fn + Delete", remoteLabel: "Delete" },
  ]
  : [
    { localLabel: "Win", localSymbol: "windows", remoteLabel: "Command", remoteSymbol: "⌘" },
    { localLabel: "Alt", remoteLabel: "Option", remoteSymbol: "⌥" },
    { localLabel: "Ctrl", remoteLabel: "Control", remoteSymbol: "⌃" },
  ],
);

type SettingsCategory = "session" | "files" | "shortcuts" | "sharingService" | "about";

const activeSettingsCategory = ref<SettingsCategory>("session");
const message = useMessage();
const sharingSettingsActive = computed(() => activeWorkspace.value === "settings" && activeSettingsCategory.value === "sharingService");
function notifySharingFeedback(content: string, tone: AppMessageTone) {
  message[tone](content, { duration: 2600, keepAliveOnHover: true });
}
useSharingStartupSettingsFeedback({
  active: sharingSettingsActive,
  sharingMode, backgroundShareOverview, t, notify: notifySharingFeedback,
});
useBackgroundPermissionRepairFeedback({
  active: computed(() => activeWorkspace.value === "localShare" || sharingSettingsActive.value),
  state: app, t, notify: notifySharingFeedback,
});

const settingsCategoryDefinitions: Array<{
  id: SettingsCategory;
  labelKey: `settings.categories.${SettingsCategory}`;
  icon: typeof CogOutline;
}> = [
  { id: "session", labelKey: "settings.categories.session", icon: CogOutline },
  { id: "files", labelKey: "settings.categories.files", icon: FolderOpenOutline },
  { id: "shortcuts", labelKey: "settings.categories.shortcuts", icon: KeypadOutline },
  { id: "sharingService", labelKey: "settings.categories.sharingService", icon: ShareSocialOutline },
  { id: "about", labelKey: "settings.categories.about", icon: InformationCircleOutline },
];

const settingsCategories = settingsCategoryDefinitions;
watch(requestedSettingsCategory, category => {
  if (category) { activeSettingsCategory.value = category; requestedSettingsCategory.value = null; }
});
watch([activeWorkspace, activeSettingsCategory], ([workspace, category]) => {
  if (workspace === 'settings' && category === 'sharingService') void refreshSharingSettings();
  else if (workspace === 'settings') cancelLocalShareStart();
});
async function serviceAction(action: WindowsServiceAction | "refresh" | "setup") {
  if (action === "refresh") await refreshLocalVncServerStatus();
  else if (action === "setup" || (action === "install" && windowsService.needsSetup.value)) await openWindowsServiceSetup();
  else await manageSharingService(action);
}

function fileAccessMessageToneClass(tone: string) {
  return `tone-${tone}`;
}

function fileAccessDirectoryStatusToneClass(status: string) {
  return status === "ready" ? "tone-success" : "tone-error";
}
</script>

<template>
<section v-show="activeWorkspace === 'settings'" class="settings-workbench">
  <nav class="settings-category-tabs" :aria-label="t('settings.title')">
    <NButton
      v-for="category in settingsCategories"
      :key="category.id"
      quaternary
      :class="['settings-category-tab', { active: activeSettingsCategory === category.id }]"
      :aria-pressed="activeSettingsCategory === category.id"
      @click="activeSettingsCategory = category.id"
    >
      <NIcon class="settings-category-icon" :component="category.icon" />
      <span>{{ t(category.labelKey) }}</span>
    </NButton>
  </nav>

  <section v-show="activeSettingsCategory === 'session'" class="settings-panel" data-settings-panel="session">
    <div class="settings-group">
      <div class="settings-field">
        <span>{{ t("sessions.postConnectPresentation") }}</span>
        <NRadioGroup
          class="settings-radio-group"
          :value="sessionSettings.connectionDefaults.postConnectPresentation"
          :aria-label="t('sessions.postConnectPresentation')"
          @update:value="setPostConnectPresentation($event as PostConnectPresentation)"
        >
          <NRadio
            v-for="option in postConnectPresentationOptions"
            :key="option"
            :value="option"
          >
            {{ t(`sessions.postConnectPresentations.${option}`) }}
          </NRadio>
        </NRadioGroup>
      </div>

      <div class="settings-toggle settings-toggle-row">
        <span>
          <strong>{{ t("settings.defaultReadonly") }}</strong>
          <small id="settings-readonly-description">{{ t("settings.readonlyDescription") }}</small>
        </span>
        <NSwitch
          :value="sessionSettings.connectionDefaults.readonly"
          :aria-label="t('settings.defaultReadonly')"
          aria-describedby="settings-readonly-description"
          @update:value="setConnectionDefault('readonly', $event)"
        />
      </div>
      <div class="settings-toggle settings-toggle-row">
        <span>
          <strong>{{ t("settings.confirmBeforeConnect") }}</strong>
          <small id="settings-confirm-description">{{ t("settings.confirmBeforeConnectDescription") }}</small>
        </span>
        <NSwitch
          :value="sessionSettings.confirmBeforeConnect"
          :aria-label="t('settings.confirmBeforeConnect')"
          aria-describedby="settings-confirm-description"
          @update:value="setConfirmBeforeConnect"
        />
      </div>

    </div>
  </section>

  <section v-show="activeSettingsCategory === 'files'" class="settings-panel" data-settings-panel="files">
    <div class="settings-group">
      <section class="settings-files-section" aria-labelledby="settings-file-transfer-title">
        <header class="settings-files-heading">
          <div class="settings-files-heading-copy">
            <h3 id="settings-file-transfer-title">{{ t("settings.fileTransferRenamePolicy") }}</h3>
            <p id="settings-file-transfer-description" class="settings-files-description">{{ t("settings.fileTransferDescription") }}</p>
          </div>
        </header>
        <NSelect
          :value="sessionSettings.fileTransferRenamePolicy"
          :options="fileTransferRenameOptions"
          :aria-label="t('settings.fileTransferRenamePolicy')"
          aria-describedby="settings-file-transfer-description"
          @update:value="setFileTransferRenamePolicy"
        />
      </section>
      <section class="settings-files-section artifact-directory-field" aria-labelledby="settings-artifact-directory-title">
        <header class="settings-files-heading">
          <div class="settings-files-heading-copy">
            <h3 id="settings-artifact-directory-title">{{ t("settings.artifactDirectory") }}</h3>
            <p id="settings-artifact-directory-description" class="settings-files-description">
              {{ t("settings.artifactDirectoryDescription") }}
            </p>
          </div>
        </header>
        <div class="artifact-directory-control">
          <div class="settings-inline-field">
            <div class="artifact-directory-select-shell">
              <NIcon v-if="artifactDirectoryOptions.length" class="settings-path-leading-icon" aria-hidden="true">
                <FolderOpenOutline />
              </NIcon>
              <NSelect
                v-if="artifactDirectoryOptions.length"
                class="artifact-directory-select"
                :value="sessionSettings.artifactDirectory || null"
                :options="artifactDirectoryOptions"
                :placeholder="t('settings.artifactDirectoryPlaceholder')"
                :aria-label="t('settings.artifactDirectory')"
                aria-describedby="settings-artifact-directory-description"
                :disabled="fileAccessDirectoryChoosing"
                @update:value="setSessionArtifactDirectory($event as string)"
              />
              <NButton
                v-else
                class="artifact-directory-empty-picker"
                :loading="fileAccessDirectoryChoosing"
                :disabled="fileAccessDirectoryChoosing"
                :title="t('settings.chooseArtifactDirectory')"
                :aria-label="t('settings.chooseArtifactDirectory')"
                @click="chooseSessionArtifactDirectory"
              >
                <template #icon><NIcon><FolderOpenOutline /></NIcon></template>
                {{ t('settings.artifactDirectoryPlaceholder') }}
              </NButton>
            </div>
            <NButton
              quaternary
              circle
              size="small"
              class="settings-icon-button"
              :loading="fileAccessDirectoryChoosing"
              :disabled="fileAccessDirectoryChoosing"
              :title="t('settings.chooseArtifactDirectory')"
              :aria-label="t('settings.chooseArtifactDirectory')"
              @click="chooseSessionArtifactDirectory"
            >
              <template #icon><NIcon><FolderOpenOutline /></NIcon></template>
            </NButton>
            <NButton
              quaternary
              circle
              size="small"
              class="settings-icon-button"
              :disabled="!canRevealSessionArtifactDirectory"
              :title="t('settings.revealArtifactDirectory')"
              :aria-label="t('settings.revealArtifactDirectory')"
              @click.stop="revealSessionArtifactDirectory"
            >
              <template #icon><NIcon><OpenOutline /></NIcon></template>
            </NButton>
          </div>
        </div>
      </section>

      <section class="settings-files-section file-access-field" aria-labelledby="settings-file-access-title">
        <header class="settings-files-heading">
          <div class="settings-files-heading-copy">
            <h3 id="settings-file-access-title">{{ t("settings.fileAccess.title") }}</h3>
            <p class="settings-files-description">{{ t("settings.fileAccess.description") }}</p>
          </div>
          <NButton
            size="small"
            :loading="fileAccessDirectoryChoosing"
            :disabled="fileAccessDirectoryChoosing"
            :title="t('settings.fileAccess.add')"
            :aria-label="t('settings.fileAccess.add')"
            @click="chooseFileAccessDirectory"
          >
            <template #icon>
              <NIcon><AddOutline /></NIcon>
            </template>
            {{ t('settings.fileAccess.add') }}
          </NButton>
        </header>
        <div class="file-access-control">
          <p
            v-if="fileAccessMessage && (fileAccessDirectories.length || fileAccessMessageTone === 'error')"
            :class="['file-access-message', fileAccessMessageToneClass(fileAccessMessageTone)]"
            :role="fileAccessMessageTone === 'error' ? 'alert' : 'status'"
          >
            {{ fileAccessMessage }}
          </p>

          <div v-if="fileAccessDirectories.length" class="file-access-list">
            <article
              v-for="directory in fileAccessDirectories"
              :key="directory.path"
              class="file-access-row"
            >
              <span class="file-access-text">
                <NIcon class="settings-path-leading-icon" aria-hidden="true">
                  <FolderOpenOutline />
                </NIcon>
                <span class="file-access-path" :title="directory.displayPath">{{ directory.displayPath }}</span>
                <span v-if="directory.isBuiltin" class="file-access-builtin">{{ t('settings.fileAccess.builtin') }}</span>
              </span>
              <span
                :class="['file-access-status', fileAccessDirectoryStatusToneClass(directory.status)]"
              >
                {{ t(`settings.fileAccess.status.${directory.status}`, { directory: directory.displayPath }) }}
              </span>
              <div class="file-access-actions">
                <NButton
                  quaternary
                  circle
                  size="small"
                  class="settings-icon-button"
                  :disabled="directory.status !== 'ready'"
                  :title="t('settings.fileAccess.reveal')"
                  :aria-label="t('settings.fileAccess.reveal')"
                  @click="revealFileAccessDirectory(directory.path)"
                >
                  <template #icon><NIcon><FolderOpenOutline /></NIcon></template>
                </NButton>
                <NButton
                  v-if="directory.isBuiltin"
                  quaternary
                  circle
                  size="small"
                  class="settings-icon-button"
                  :disabled="fileAccessDirectoryChoosing"
                  :title="t('settings.fileAccess.changeBuiltin')"
                  :aria-label="t('settings.fileAccess.changeBuiltin')"
                  @click="chooseBuiltinFileAccessDirectory"
                >
                  <template #icon>
                    <NIcon><CreateOutline /></NIcon>
                  </template>
                </NButton>
                <NButton
                  v-else
                  quaternary
                  circle
                  size="small"
                  class="settings-icon-button"
                  :title="t('settings.fileAccess.remove')"
                  :aria-label="t('settings.fileAccess.remove')"
                  @click="revokeFileAccessDirectory(directory.path)"
                >
                  <template #icon>
                    <NIcon><TrashOutline /></NIcon>
                  </template>
                </NButton>
              </div>
            </article>
          </div>
          <NEmpty
            v-else
            size="small"
            class="file-access-empty"
            :description="t('settings.fileAccess.empty')"
          />
        </div>
      </section>
    </div>
  </section>

  <section v-show="activeSettingsCategory === 'shortcuts'" class="settings-panel" data-settings-panel="shortcuts">
    <section class="settings-shortcut-group" aria-labelledby="settings-session-shortcuts">
      <h3 id="settings-session-shortcuts" class="settings-group-heading">{{ t('settings.sessionShortcutTitle') }}</h3>
      <div
        v-for="row in sessionShortcutRows"
        :key="row.actionId"
        class="settings-shortcut-row"
      >
        <span class="settings-shortcut-name">{{ t(row.nameKey) }}</span>
        <kbd class="settings-shortcut-keys" :title="row.fullLabel" :aria-label="row.fullLabel">{{ row.label }}</kbd>
      </div>
      <p class="settings-shortcut-description">{{ t('settings.keyboardTransmission.captureDescription') }}</p>
    </section>
    <section class="settings-shortcut-group" aria-labelledby="settings-keyboard-transmission">
      <h3 id="settings-keyboard-transmission" class="settings-group-heading">{{ t("settings.keyboardTransmission.title") }}</h3>
      <table class="settings-keyboard-table">
        <thead>
          <tr>
            <th scope="col">{{ t('settings.keyboardTransmission.localKey') }}</th>
            <th scope="col">{{ t('settings.keyboardTransmission.sentKey') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in keyboardTransmissionRows" :key="row.localLabel">
            <td>
              <span class="settings-shortcut-name settings-keyboard-local-key">
                <template v-if="row.localSymbol === 'windows'">
                  <svg class="settings-windows-key-icon" viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M3 4.5 10.5 3v8H3V4.5Zm9 6.5V2.7L21 1v10h-9Zm-9 2H10.5v8L3 19.5V13Zm9 0h9v10l-9-1.7V13Z" />
                  </svg>
                </template>
                <span v-else-if="row.localSymbol" class="settings-key-symbol">{{ row.localSymbol }}</span>
                {{ row.localLabel }}
              </span>
            </td>
            <td>
              <span class="settings-keyboard-sent-key">
                <template v-if="row.remoteSymbol === 'windows'">
                  <svg class="settings-windows-key-icon" viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M3 4.5 10.5 3v8H3V4.5Zm9 6.5V2.7L21 1v10h-9Zm-9 2H10.5v8L3 19.5V13Zm9 0h9v10l-9-1.7V13Z" />
                  </svg>
                </template>
                <span v-else-if="row.remoteSymbol" class="settings-key-symbol">{{ row.remoteSymbol }}</span>
                {{ row.remoteLabel }}
              </span>
            </td>
          </tr>
        </tbody>
      </table>
    </section>
  </section>

  <section v-show="activeSettingsCategory === 'sharingService'" class="settings-panel" data-settings-panel="sharingService">
    <LocalShareSecuritySettings
      :active="sharingSettingsActive"
      :t="t"
    />
    <div class="settings-group">
      <div class="settings-toggle settings-toggle-row">
        <span>
          <strong>{{ t(sharingMode === 'windowsService' ? 'localShare.windowsService.autoStart' : 'localShare.background.autoStart') }}</strong>
          <small v-if="startupRegistrationNeedsAction" class="sharing-startup-warning" role="alert">{{ startupRegistrationHint }}</small>
          <small v-if="startupAttemptFailed" class="sharing-startup-warning" role="alert">{{ t('settings.sharing.startupFailed') }}</small>
        </span>
        <div class="sharing-startup-actions">
          <NSwitch
            :value="sharingAutoStart"
            :loading="sharingAutoStartBusy"
            :disabled="sharingAutoStartBusy || !sharingAutoStartKnown"
            :aria-label="t(sharingMode === 'windowsService' ? 'localShare.windowsService.autoStart' : 'localShare.background.autoStart')"
            @update:value="setSharingAutoStart"
          />
          <NButton
            quaternary
            circle
            size="small"
            class="settings-icon-button"
            :title="t('settings.sharing.setPassword')"
            :aria-label="t('settings.sharing.setPassword')"
            @click="selectLocalShareWorkspace"
          >
            <template #icon><NIcon><OpenOutline /></NIcon></template>
          </NButton>
        </div>
      </div>
      <details class="sharing-startup-details">
        <summary>{{ t('settings.sharing.startupDetails') }}</summary>
        <p v-if="sharingMode === 'windowsService'">{{ t('settings.sharing.adminRequired') }}</p>
        <p v-if="startupRegistrationHint && !startupRegistrationNeedsAction">{{ startupRegistrationHint }}</p>
        <p>{{ startupAttemptText }}</p>
      </details>
    </div>
    <WindowsServiceSettings
      v-if="windowsService.overview.value?.supported"
      :overview="windowsService.overview.value"
      :background-overview="backgroundShareOverview"
      :busy="windowsService.busy.value || localVncServerLoading"
      :repairing="windowsService.repairing.value"
      :repair-focus="requestedSharingRepair"
      :repair-items="windowsService.repairItems.value"
      :running="windowsService.running.value"
      :background-running="backgroundShareOverview?.server.status === 'running'"
      :active="sharingSettingsActive"
      :t="t"
      @action="serviceAction"
    />
    <MacSharingSettings
      v-if="backgroundShareOverview?.restartSupported"
      :busy="localVncServerLoading || localShareStartPending"
      :can-maintain="backgroundRestartSupported"
      :outdated="backgroundRuntimeOutdated"
      :permission-repair-needed="backgroundPermissionRepairNeeded"
      :check-error="localSharePermissionCheckError || sharingRuntimeError"
      :runtime-error-code="backgroundShareOverview.lastErrorCode"
      :active="sharingSettingsActive"
      :repair-focus="requestedSharingRepair"
      :t="t"
      @restart="restartBackgroundShare"
      @repair="updateAndRepairBackgroundShare"
    />
  </section>

  <section v-show="activeSettingsCategory === 'about'" class="settings-panel" data-settings-panel="about">
    <div class="settings-group settings-about-group">
      <div class="settings-about-hero">
        <img class="settings-about-icon" :src="appIcon" alt="" width="48" height="48" />
        <div>
          <h2>{{ appMetadata.name }}</h2>
          <p class="settings-about-version">{{ t("settings.about.versionLabel") }} {{ appMetadata.version }}</p>
        </div>
      </div>

      <DistributionInformation :active="activeWorkspace === 'settings' && activeSettingsCategory === 'about'" />
      <details v-if="showBuildRuntimeInfo" class="settings-about-debug">
        <summary>{{ t('settings.about.debugInfoTitle') }}</summary>
        <dl class="settings-about-list">
          <div>
            <dt>{{ t("settings.about.buildLabel") }}</dt>
            <dd>{{ appBuildLabel }}</dd>
          </div>
          <div>
            <dt>{{ t("settings.about.platformLabel") }}</dt>
            <dd>{{ platformLabel }}</dd>
          </div>
          <div>
            <dt>{{ t("settings.about.runModeLabel") }}</dt>
            <dd>{{ runModeLabel }}</dd>
          </div>
          <div>
            <dt>{{ t("settings.about.tauriBackendLabel") }}</dt>
            <dd>{{ tauriBackendStatus }}</dd>
          </div>
        </dl>
      </details>
    </div>
  </section>
</section>
</template>
