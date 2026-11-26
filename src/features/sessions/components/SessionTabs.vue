<script setup lang="ts">
import {
  CloseOutline,
  ContractOutline,
  EnterOutline,
  ExpandOutline,
  OpenOutline,
  PowerOutline,
} from "@vicons/ionicons5";
import { NButton, NEmpty, NIcon } from "naive-ui";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";

const {
  activateSessionTab,
  closeSessionTab,
  disconnectSessionFromViewport,
  fullscreenSessionId,
  externalWindowLocation,
  focusExternalWindowSession,
  getRemoteViewportSummary,
  getWorkspaceRenderSlot,
  sessionTabs,
  t,
  toggleExternalWindowSession,
  toggleSessionFullscreen,
} = useVncAppContext();

function isCurrentExternalSession(tabId: string) {
  return externalWindowLocation?.sessionId === tabId;
}

function shouldShowExternalWindowAction(tabId: string) {
  const slot = getWorkspaceRenderSlot(tabId);
  return isCurrentExternalSession(tabId) || slot.externalWindowCandidate || slot.externalWindowOwned;
}

function getExternalWindowActionLabel(tabId: string) {
  const slot = getWorkspaceRenderSlot(tabId);
  return isCurrentExternalSession(tabId) || slot.externalWindowOwned
    ? t("sessions.dockWindow")
    : t("sessions.detachWindow");
}

function handleSessionTabClick(tabId: string) {
  if (getWorkspaceRenderSlot(tabId).externalWindowOwned) {
    void focusExternalWindowSession(tabId);
    return;
  }

  void activateSessionTab(tabId);
}
</script>

<template>
  <section class="session-rail-content" aria-label="Session tabs">
    <header class="session-rail-header">
      <span>{{ t("sessions.railTitle") }}</span>
      <strong>{{ sessionTabs.length }}</strong>
    </header>

    <section v-if="sessionTabs.length" class="session-tabs">
      <div
        v-for="tab in sessionTabs"
        :key="tab.id"
        :class="['session-tab', { active: tab.active }]"
        @click="handleSessionTabClick(tab.id)"
      >
        <NButton text class="session-tab-main" @click.stop="handleSessionTabClick(tab.id)">
          <span class="session-tab-title">
            <span
              class="session-tab-dot"
              :data-tone="getRemoteViewportSummary(tab).tone"
            ></span>
            <span class="session-tab-label" :title="tab.railLabel ?? tab.label">{{ tab.railLabel ?? tab.label }}</span>
          </span>
          <span class="session-tab-state" :title="t(`connection.${getRemoteViewportSummary(tab).statusLabel}`)">
            <span>{{ t(`connection.${getRemoteViewportSummary(tab).statusLabel}`) }}</span>
            <span v-if="getWorkspaceRenderSlot(tab.id).externalWindowOwned"> · {{ t("sessions.detachWindow") }}</span>
          </span>
        </NButton>
        <div class="session-tab-actions">
          <NButton
            v-if="getRemoteViewportSummary(tab).canDisconnect"
            quaternary
            circle
            size="small"
            class="session-tab-icon-button"
            :aria-label="`${t('toolbar.disconnect')} ${tab.railLabel ?? tab.label}`"
            :title="t('toolbar.disconnect')"
            @click.stop="disconnectSessionFromViewport(tab.id)"
          >
            <NIcon :size="14" :component="PowerOutline" aria-hidden="true" />
          </NButton>
          <NButton
            v-if="shouldShowExternalWindowAction(tab.id)"
            quaternary
            circle
            size="small"
            class="session-tab-icon-button"
            :aria-label="`${getExternalWindowActionLabel(tab.id)} ${tab.railLabel ?? tab.label}`"
            :title="getExternalWindowActionLabel(tab.id)"
            @click.stop="toggleExternalWindowSession(tab.id)"
          >
            <NIcon
              :size="14"
              :component="isCurrentExternalSession(tab.id) || getWorkspaceRenderSlot(tab.id).externalWindowOwned ? EnterOutline : OpenOutline"
              aria-hidden="true"
            />
          </NButton>
          <NButton
            v-if="getRemoteViewportSummary(tab).canFullscreen"
            quaternary
            circle
            size="small"
            class="session-tab-icon-button"
            :aria-label="`${fullscreenSessionId === tab.id ? t('sessions.exitFullscreen') : t('sessions.fullscreen')} ${tab.railLabel ?? tab.label}`"
            :title="fullscreenSessionId === tab.id ? t('sessions.exitFullscreen') : t('sessions.fullscreen')"
            @click.stop="toggleSessionFullscreen(tab.id)"
          >
            <NIcon
              :size="14"
              :component="fullscreenSessionId === tab.id ? ContractOutline : ExpandOutline"
              aria-hidden="true"
            />
          </NButton>
          <NButton
            v-if="getRemoteViewportSummary(tab).canClose"
            quaternary
            circle
            size="small"
            type="error"
            class="session-tab-icon-button danger"
            :aria-label="`${t('tabs.close')} ${tab.railLabel ?? tab.label}`"
            :title="t('tabs.close')"
            @click.stop="closeSessionTab(tab.id)"
          >
            <NIcon :size="14" :component="CloseOutline" aria-hidden="true" />
          </NButton>
        </div>
      </div>
    </section>

    <NEmpty
      v-else
      class="session-rail-empty"
      :description="t('sessions.railEmpty')"
    />
  </section>
</template>
