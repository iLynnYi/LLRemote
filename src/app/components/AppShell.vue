<script setup lang="ts">
import { useVncAppContext } from "../composables/useVncAppContext.ts";
import ModuleRail from "./ModuleRail.vue";
import ConfirmDialog from "./ConfirmDialog.vue";
import ConnectionDialog from "./ConnectionDialog.vue";
import PasswordPromptDialog from "./PasswordPromptDialog.vue";
import HostWorkbench from "../../features/hosts/components/HostWorkbench.vue";
import LocalShareWorkbench from "../../features/local-share/components/LocalShareWorkbench.vue";
import LocalSharePermissionGuide from "../../features/local-share/components/LocalSharePermissionGuide.vue";
import SessionWorkbench from "../../features/sessions/components/SessionWorkbench.vue";
import SettingsWorkbench from "../../features/settings/components/SettingsWorkbench.vue";
import GlobalTransferDrawer from "../../features/transfers/components/GlobalTransferDrawer.vue";

const {
  activeWorkspace,
  connectionConfirmDraft,
  confirmDialog,
  fullscreenSessionId,
  passwordPrompt,
} = useVncAppContext();
</script>

<template>
  <main
    :class="[
      'app-shell',
      {
        'viewport-fullscreen-active': fullscreenSessionId,
      },
    ]"
  >
    <ModuleRail />

    <section class="workspace">
      <HostWorkbench v-show="activeWorkspace === 'hosts'" />
      <LocalShareWorkbench v-show="activeWorkspace === 'localShare'" />
      <SettingsWorkbench v-show="activeWorkspace === 'settings'" />
      <SessionWorkbench v-show="activeWorkspace === 'sessions'" />
    </section>

    <ConnectionDialog v-if="connectionConfirmDraft" />
    <LocalSharePermissionGuide />
    <PasswordPromptDialog v-if="passwordPrompt.visible" />
    <ConfirmDialog v-if="confirmDialog.visible" />
    <GlobalTransferDrawer />
  </main>
</template>
