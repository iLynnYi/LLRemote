<script setup lang="ts">
import {
  DesktopOutline,
  SettingsOutline,
  ShareSocialOutline,
  TerminalOutline,
} from "@vicons/ionicons5";
import { NButton, NIcon } from "naive-ui";
import appIcon from "../../../src-tauri/icons/icon-source.svg";
import { useVncAppContext } from "../composables/useVncAppContext.ts";
import GlobalTransferStatus from "../../features/transfers/components/GlobalTransferStatus.vue";

const {
  activeWorkspace,
  selectLocalShareWorkspace,
  selectWorkspace,
  t,
} = useVncAppContext();
</script>

<template>
<aside class="module-rail" aria-label="Application modules">
  <img class="rail-mark" :src="appIcon" :title="t('app.description')" :alt="t('app.title')" />
  <nav class="rail-nav">
    <NButton
      quaternary
      :class="['rail-item', { active: activeWorkspace === 'hosts' }]"
      :title="t('modules.hosts')"
      @click="selectWorkspace('hosts')"
    >
      <NIcon class="rail-glyph" :component="DesktopOutline" />
      <strong>{{ t("modules.hosts") }}</strong>
    </NButton>
    <NButton
      quaternary
      :class="['rail-item', { active: activeWorkspace === 'sessions' }]"
      :title="t('modules.sessions')"
      @click="selectWorkspace('sessions')"
    >
      <NIcon class="rail-glyph" :component="TerminalOutline" />
      <strong>{{ t("modules.sessions") }}</strong>
    </NButton>
    <NButton
      quaternary
      :class="['rail-item', { active: activeWorkspace === 'localShare' }]"
      :title="t('modules.localShare')"
      @click="selectLocalShareWorkspace"
    >
      <NIcon class="rail-glyph" :component="ShareSocialOutline" />
      <strong>{{ t("modules.localShare") }}</strong>
    </NButton>
    <GlobalTransferStatus variant="rail" />
    <NButton
      quaternary
      :class="['rail-item', { active: activeWorkspace === 'settings' }]"
      :title="t('modules.settings')"
      @click="selectWorkspace('settings')"
    >
      <NIcon class="rail-glyph" :component="SettingsOutline" />
      <strong>{{ t("modules.settings") }}</strong>
    </NButton>
  </nav>
</aside>
</template>
