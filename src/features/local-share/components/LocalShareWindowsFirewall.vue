<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { NButton, useThemeVars } from "naive-ui";
import type { Translate } from "../../../app/types.ts";
import { useWindowsFirewall } from "../composables/useWindowsFirewall.ts";

const props = defineProps<{ active: boolean; t: Translate }>();
const theme = useThemeVars();
const { profiles, checking, openingSettings, error, settingsError, refresh, openSettings, markSettingsDeparture, refreshAfterSettings } = useWindowsFirewall({
  active: computed(() => props.active), invoke, t: props.t,
});
let entered = false;
onMounted(() => {
  watch(() => props.active, active => {
    if (active && !entered) { entered = true; void refresh(); }
    else if (active) void refreshAfterSettings();
  }, { immediate: true });
  window.addEventListener("blur", markSettingsDeparture);
  window.addEventListener("focus", refreshOnReturn);
  document.addEventListener("visibilitychange", visibilityChanged);
});
function refreshOnReturn() { if (document.visibilityState === "visible") void refreshAfterSettings(); }
function visibilityChanged() {
  if (document.visibilityState === "hidden") markSettingsDeparture();
  else refreshOnReturn();
}
onBeforeUnmount(() => {
  window.removeEventListener("blur", markSettingsDeparture);
  window.removeEventListener("focus", refreshOnReturn);
  document.removeEventListener("visibilitychange", visibilityChanged);
});
defineExpose({ refresh });
</script>

<template>
  <div class="local-share-permissions">
    <div class="local-share-panel-head"><h3>{{ t('localShare.permissions.title') }}</h3></div>
    <div class="firewall-card" :aria-busy="checking">
      <div class="firewall-heading">
        <strong class="firewall-title">{{ t('localShare.firewall.title') }}</strong>
        <NButton class="firewall-settings" size="small" :loading="openingSettings" :disabled="openingSettings" @click="openSettings">{{ t('localShare.firewall.openSettings') }}</NButton>
      </div>
      <dl v-if="profiles.length" class="firewall-networks" role="status" aria-live="polite" :title="error || undefined">
        <div v-for="profile in profiles" :key="profile.name" class="firewall-network">
          <dt>{{ t(`localShare.firewall.network.${profile.name}`) }}：</dt>
          <dd class="firewall-connection">{{ t(`localShare.firewall.connection.${profile.connected === null ? (checking ? 'checking' : 'unknown') : profile.connected ? 'connected' : 'disconnected'}`) }}<span class="firewall-separator" aria-hidden="true">·</span></dd>
          <dd :class="['firewall-enabled', { enabled: profile.enabled === true, disabled: profile.enabled === false }]">
            {{ t(`localShare.firewall.state.${profile.enabled === null ? (checking ? 'checking' : 'unknown') : profile.enabled ? 'enabled' : 'disabled'}`) }}
          </dd>
        </div>
      </dl>
      <p v-else class="firewall-empty" role="status" :title="error || undefined">{{ t(checking ? 'localShare.firewall.readingNetworks' : 'localShare.firewall.networksUnavailable') }}</p>
      <p v-if="settingsError" class="firewall-error" role="alert">{{ settingsError }}</p>
    </div>
  </div>
</template>

<style scoped>
.local-share-panel-head h3 { color: v-bind('theme.textColor1'); }
.firewall-card { display: grid; gap: 8px; min-width: 0; padding: 8px 12px; border: 1px solid v-bind('theme.borderColor'); border-radius: 8px; background: v-bind('theme.actionColor'); }
.firewall-heading { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.firewall-title { font-size: 13px; color: v-bind('theme.textColor1'); }
.firewall-networks { display: flex; flex-wrap: wrap; gap: 6px 24px; margin: 0; font-size: 12px; line-height: 1.5; }
.firewall-network { display: inline-flex; align-items: baseline; flex-wrap: nowrap; white-space: nowrap; }
.firewall-network dt { color: v-bind('theme.textColor2'); }
.firewall-network dd { margin: 0; color: v-bind('theme.textColor3'); }
.firewall-separator { margin: 0 6px; color: v-bind('theme.textColor3'); }
.firewall-network .firewall-enabled.enabled { color: v-bind('theme.successColor'); }
.firewall-network .firewall-enabled.disabled { color: v-bind('theme.warningColor'); }
.firewall-settings { flex-shrink: 0; }
.firewall-empty { margin: 0; font-size: 12px; line-height: 1.5; color: v-bind('theme.textColor3'); }
.firewall-error { margin: 0; font-size: 12px; color: v-bind('theme.errorColor'); }
</style>
