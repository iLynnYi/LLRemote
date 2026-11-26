import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  findKeyboardShortcutConflict,
  shortcutFromKeyboardEvent,
  validateKeyboardShortcut,
  type KeyboardShortcutConflict,
} from "@session/input/keyboardShortcut";
import {
  updateSessionSettings,
  type ClipboardSyncMode,
  type SessionSettings,
  type FileTransferRenamePolicy,
} from "@session/settings/sessionSettings";
import type { PostConnectPresentation } from "@session/core/connectionConfirm";
import {
  loadSessionSettings,
  saveSessionSettings,
  type SessionSettingsStorage,
} from "@session/settings/sessionSettingsStore";
import { browserSessionSettingsStorage } from "@app/utils/sessionSettingsStorage";
import type { Translate } from "@app/types";
import { resolveArtifactDirectorySelection } from "@session/settings/artifactDirectorySelection";

type UseSettingsWorkbenchOptions = {
  t: Translate;
  getGlobalKeyboardShortcutRecords: () => KeyboardShortcutConflict[];
  notify: (content: string, tone: "success" | "error") => void;
};

type ArtifactDirectoryStatus = {
  directory: string;
  status: "empty" | "missing" | "not_directory" | "not_writable" | "ready";
  exists: boolean;
  isDirectory: boolean;
  writable: boolean;
  message: string;
};

type FileAccessDirectory = {
  path: string;
  addedAt: string;
  isBuiltin: boolean;
  status: "missing" | "not_directory" | "not_writable" | "ready";
  exists: boolean;
  isDirectory: boolean;
  writable: boolean;
  message: string;
};

type FileAccessActionResult = {
  status: "opened" | "authorized" | "needs_authorization" | "invalid_path";
  path: string;
  directory: string;
  message: string;
};

// 仅转换展示文本；授权、存储和系统调用继续使用原始规范化路径。
function formatDirectoryDisplayPath(path: string): string {
  const prefix = "\\\\?\\";
  if (path.slice(0, 8).toUpperCase() === prefix + "UNC\\") {
    return "\\\\" + path.slice(8);
  }
  const drivePath = path.slice(4);
  return path.startsWith(prefix) && /^[a-z]:\\/i.test(drivePath) ? drivePath : path;
}

export function useSettingsWorkbench(options: UseSettingsWorkbenchOptions) {
  const { t, getGlobalKeyboardShortcutRecords, notify } = options;
  const sessionSettingsStorage: SessionSettingsStorage | undefined = browserSessionSettingsStorage();
  const sessionSettings = ref<SessionSettings>(loadSessionSettings(sessionSettingsStorage));
  const fullscreenExitShortcutError = ref("");
  const sessionArtifactDirectoryMessage = ref("");
  const sessionArtifactDirectoryStatus = ref<ArtifactDirectoryStatus | null>(null);
  const fileAccessDirectories = ref<FileAccessDirectory[]>([]);
  const fileAccessMessage = ref("");
  const fileAccessMessageTone = ref<"default" | "success" | "error">("default");
  const fileAccessDirectoryChoosing = ref(false);

  const currentFullscreenExitShortcut = computed(() => sessionSettings.value.fullscreenExitShortcut);
  const sessionArtifactDirectoryDisplayPath = computed(() => formatDirectoryDisplayPath(sessionSettings.value.artifactDirectory));
  const sessionArtifactDirectoryStatusTone = computed(() => {
    if (!sessionSettings.value.artifactDirectory) {
      return "default";
    }
    return sessionArtifactDirectoryStatus.value?.status === "ready" ? "success" : "error";
  });
  const canRevealSessionArtifactDirectory = computed(() =>
    sessionArtifactDirectoryStatus.value?.status === "ready"
      && sessionArtifactDirectoryStatus.value.directory === sessionSettings.value.artifactDirectory,
  );
  const artifactDirectoryOptions = computed(() =>
    fileAccessDirectories.value.map((directory) => ({
      label: formatDirectoryDisplayPath(directory.path),
      value: directory.path,
    })),
  );
  const fileAccessDirectoryRows = computed(() =>
    fileAccessDirectories.value.map(directory => ({
      ...directory,
      displayPath: formatDirectoryDisplayPath(directory.path),
    })),
  );
  const unavailableFileAccessDirectories = computed(() =>
    fileAccessDirectories.value.filter((directory) => directory.status !== "ready"),
  );

  function persistSessionSettings() {
    saveSessionSettings(sessionSettingsStorage, sessionSettings.value);
  }

  function setConfirmBeforeConnect(value: boolean) {
    sessionSettings.value = updateSessionSettings(sessionSettings.value, { confirmBeforeConnect: value });
    persistSessionSettings();
  }

  function setClipboardSyncMode(value: ClipboardSyncMode) {
    sessionSettings.value = updateSessionSettings(sessionSettings.value, { clipboardSyncMode: value });
    persistSessionSettings();
  }

  function setFileTransferRenamePolicy(value: FileTransferRenamePolicy) {
    const next = updateSessionSettings(sessionSettings.value, { fileTransferRenamePolicy: value });
    try {
      if (!saveSessionSettings(sessionSettingsStorage, next)) throw new Error("storage_unavailable");
      sessionSettings.value = next;
    } catch {
      notify(t("settings.fileTransferSaveFailed"), "error");
    }
  }

  async function inspectSessionArtifactDirectory(value: string) {
    const directory = value.trim();
    if (!directory) {
      sessionArtifactDirectoryStatus.value = null;
      sessionArtifactDirectoryMessage.value = "";
      return;
    }

    try {
      const status = await invoke("inspect_session_artifact_directory", { directory }) as ArtifactDirectoryStatus;
      if (sessionSettings.value.artifactDirectory !== directory) return;
      sessionArtifactDirectoryStatus.value = status;
      sessionArtifactDirectoryMessage.value = t(`settings.artifactDirectoryStatus.${status.status}`, {
        directory: formatDirectoryDisplayPath(status.directory),
        message: status.message,
      });
    } catch (error) {
      if (sessionSettings.value.artifactDirectory !== directory) return;
      sessionArtifactDirectoryStatus.value = null;
      sessionArtifactDirectoryMessage.value = t("settings.artifactDirectoryInspectFailed", { error: String(error) });
    }
  }

  async function setSessionArtifactDirectory(value: string) {
    const directory = fileAccessDirectories.value.find(entry => entry.path === value);
    if (!directory) return;
    const next = updateSessionSettings(sessionSettings.value, {
      artifactDirectory: directory.path, artifactDirectoryMode: directory.isBuiltin ? "builtin" : "custom",
    });
    try {
      if (!saveSessionSettings(sessionSettingsStorage, next)) throw new Error("storage_unavailable");
    } catch {
      notify(t("settings.artifactDirectorySaveFailed"), "error");
      return;
    }
    sessionSettings.value = next;
    await inspectSessionArtifactDirectory(sessionSettings.value.artifactDirectory);
  }

  async function revealSessionArtifactDirectory() {
    const directory = sessionSettings.value.artifactDirectory.trim();
    if (!directory || !canRevealSessionArtifactDirectory.value) {
      sessionArtifactDirectoryMessage.value = t("settings.artifactDirectoryRevealUnavailable");
      return;
    }

    try {
      const result = await invoke("reveal_authorized_path", { request: { path: directory } }) as FileAccessActionResult;
      if (result.status === "needs_authorization") {
        await grantFileAccessDirectory(result.directory);
        const retry = await invoke("reveal_authorized_path", { request: { path: directory } }) as FileAccessActionResult;
        if (retry.status !== "opened") {
          throw new Error(retry.message);
        }
        return;
      }
      if (result.status !== "opened") {
        throw new Error(result.message);
      }
    } catch (error) {
      sessionArtifactDirectoryMessage.value = t("settings.artifactDirectoryRevealFailed", { error: String(error) });
    }
  }

  async function loadFileAccessDirectories() {
    try {
      fileAccessDirectories.value = await invoke("list_file_access_directories") as FileAccessDirectory[];
      const selection = resolveArtifactDirectorySelection(sessionSettings.value, fileAccessDirectories.value);
      let saveFailed = false;
      if (selection.artifactDirectory !== sessionSettings.value.artifactDirectory
        || selection.artifactDirectoryMode !== sessionSettings.value.artifactDirectoryMode) {
        // The backend already committed grants. Publish its current default even if caching fails.
        sessionSettings.value = updateSessionSettings(sessionSettings.value, selection);
        try {
          if (!saveSessionSettings(sessionSettingsStorage, sessionSettings.value)) throw new Error("storage_unavailable");
        } catch {
          saveFailed = true;
          fileAccessMessage.value = t("settings.artifactDirectorySaveFailed");
          fileAccessMessageTone.value = "error";
        }
      }
      await inspectSessionArtifactDirectory(sessionSettings.value.artifactDirectory);
      if (saveFailed) return;
      if (!fileAccessDirectories.value.length) {
        fileAccessMessage.value = t("settings.fileAccess.empty");
        fileAccessMessageTone.value = "default";
      } else if (unavailableFileAccessDirectories.value.length) {
        fileAccessMessage.value = t("settings.fileAccess.unavailableCount", {
          count: unavailableFileAccessDirectories.value.length,
        });
        fileAccessMessageTone.value = "error";
      } else {
        fileAccessMessage.value = "";
        fileAccessMessageTone.value = "default";
      }
    } catch (error) {
      fileAccessMessage.value = error === "file_access_settings_invalid"
        ? t("settings.fileAccess.settingsInvalid")
        : error === "file_access_settings_unavailable"
          ? t("settings.fileAccess.settingsUnavailable")
          : t("settings.fileAccess.loadFailed", { error: String(error) });
      fileAccessMessageTone.value = "error";
    }
  }

  async function grantFileAccessDirectory(directory: string) {
    const trimmed = directory.trim();
    if (!trimmed) {
      return;
    }

    try {
      const granted = await invoke<Pick<FileAccessDirectory, "path" | "addedAt">>("grant_file_access_directory", { request: { directory: trimmed } });
      await loadFileAccessDirectories();
      if (!sessionSettings.value.artifactDirectory) {
        await setSessionArtifactDirectory(granted.path);
      }
      return granted;
    } catch {
      fileAccessMessage.value = t("settings.fileAccess.grantFailed");
      fileAccessMessageTone.value = "error";
    }
  }

  async function chooseFileAccessDirectory() {
    if (fileAccessDirectoryChoosing.value) return;
    fileAccessDirectoryChoosing.value = true;
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });

      if (typeof selected === "string") {
        return await grantFileAccessDirectory(selected);
      }
    } catch {
      fileAccessMessage.value = t("settings.fileAccess.chooseFailed");
      fileAccessMessageTone.value = "error";
    } finally {
      fileAccessDirectoryChoosing.value = false;
    }
  }

  // 保存位置采用后端返回的规范化路径，与授权列表保持同一标识。
  async function chooseSessionArtifactDirectory() {
    const granted = await chooseFileAccessDirectory();
    if (granted && sessionSettings.value.artifactDirectory !== granted.path) {
      await setSessionArtifactDirectory(granted.path);
    }
  }

  async function chooseBuiltinFileAccessDirectory() {
    if (fileAccessDirectoryChoosing.value) return;
    fileAccessDirectoryChoosing.value = true;
    let selected = false;
    try {
      const builtin = fileAccessDirectories.value.find(entry => entry.isBuiltin);
      const directory = await open({
        directory: true, multiple: false,
        defaultPath: builtin?.status === "ready" ? builtin.path : undefined,
      });
      if (typeof directory !== "string") return;
      selected = true;
      await invoke<Pick<FileAccessDirectory, "path">>("set_builtin_file_access_directory", {
        request: { directory },
      });
      await loadFileAccessDirectories();
    } catch {
      fileAccessMessage.value = t(selected ? "settings.fileAccess.builtinUpdateFailed" : "settings.fileAccess.chooseFailed");
      fileAccessMessageTone.value = "error";
    } finally {
      fileAccessDirectoryChoosing.value = false;
    }
  }

  async function revokeFileAccessDirectory(directory: string) {
    if (fileAccessDirectories.value.some(entry => entry.path === directory && entry.isBuiltin)) {
      notify(t("settings.fileAccess.builtinCannotRemove"), "error");
      return;
    }
    const wasSelected = sessionSettings.value.artifactDirectory === directory;
    try {
      await invoke("revoke_file_access_directory", { request: { directory } });
      await loadFileAccessDirectories();
      if (wasSelected) {
        sessionArtifactDirectoryMessage.value = t("settings.artifactDirectoryRevoked");
      }
    } catch (error) {
      notify(t("settings.fileAccess.revokeFailed", { error: String(error) }), "error");
    }
  }

  async function revealFileAccessDirectory(directory: string) {
    const entry = fileAccessDirectories.value.find((item) => item.path === directory);
    if (entry && entry.status !== "ready") {
      notify(t(`settings.fileAccess.status.${entry.status}`, { directory: formatDirectoryDisplayPath(entry.path) }), "error");
      return;
    }

    try {
      const result = await invoke("reveal_authorized_path", { request: { path: directory } }) as FileAccessActionResult;
      if (result.status !== "opened") {
        throw new Error(result.message);
      }
    } catch (error) {
      notify(t("settings.fileAccess.revealFailed", { error: String(error) }), "error");
    }
  }

  function setFullscreenExitShortcut(value: string): boolean {
    const validation = validateKeyboardShortcut(value);
    if (!validation.valid) {
      fullscreenExitShortcutError.value = t(`settings.shortcutErrors.${validation.reason}`);
      return false;
    }

    const conflict = findKeyboardShortcutConflict(
      validation.shortcut,
      getGlobalKeyboardShortcutRecords(),
      "fullscreenExit",
    );
    if (conflict) {
      fullscreenExitShortcutError.value = t("settings.shortcutErrors.conflict", {
        name: t(conflict.labelKey),
      });
    } else {
      fullscreenExitShortcutError.value = "";
    }

    sessionSettings.value = updateSessionSettings(sessionSettings.value, {
      fullscreenExitShortcut: validation.shortcut,
    });
    persistSessionSettings();
    return true;
  }

  function captureFullscreenExitShortcut(event: KeyboardEvent) {
    event.preventDefault();
    event.stopPropagation();
    setFullscreenExitShortcut(shortcutFromKeyboardEvent(event));
  }

  function setConnectionDefault(option: Exclude<keyof SessionSettings["connectionDefaults"], "postConnectPresentation">, value: boolean) {
    sessionSettings.value = updateSessionSettings(sessionSettings.value, {
      connectionDefaults: {
        [option]: value,
      },
    });
    persistSessionSettings();
  }

  function setPostConnectPresentation(value: PostConnectPresentation) {
    sessionSettings.value = updateSessionSettings(sessionSettings.value, {
      connectionDefaults: {
        postConnectPresentation: value,
      },
    });
    persistSessionSettings();
  }

  void inspectSessionArtifactDirectory(sessionSettings.value.artifactDirectory);
  void loadFileAccessDirectories();

  return {
    captureFullscreenExitShortcut,
    artifactDirectoryOptions,
    canRevealSessionArtifactDirectory,
    chooseFileAccessDirectory,
    chooseBuiltinFileAccessDirectory,
    chooseSessionArtifactDirectory,
    currentFullscreenExitShortcut,
    fileAccessDirectories: fileAccessDirectoryRows,
    fileAccessDirectoryChoosing,
    fileAccessMessage,
    fileAccessMessageTone,
    fullscreenExitShortcutError,
    persistSessionSettings,
    revealSessionArtifactDirectory,
    revokeFileAccessDirectory,
    revealFileAccessDirectory,
    sessionSettings,
    sessionArtifactDirectoryDisplayPath,
    sessionArtifactDirectoryMessage,
    sessionArtifactDirectoryStatus,
    sessionArtifactDirectoryStatusTone,
    sessionSettingsStorage,
    setClipboardSyncMode,
    setConfirmBeforeConnect,
    setConnectionDefault,
    setPostConnectPresentation,
    setFullscreenExitShortcut,
    setSessionArtifactDirectory,
    setFileTransferRenamePolicy,
  };
}
