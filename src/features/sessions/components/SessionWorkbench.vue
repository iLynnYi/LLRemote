<script setup lang="ts">
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";
import SessionDiagnosticsPanel from "./SessionDiagnosticsPanel.vue";
import SessionTabs from "./SessionTabs.vue";
import SessionToolbar from "./SessionToolbar.vue";
import SessionTopbar from "./SessionTopbar.vue";
import SessionViewportGrid from "./SessionViewportGrid.vue";

const {
  activeWorkspace,
  diagnosticsCollapsed,
} = useVncAppContext();
</script>

<template>
  <section v-show="activeWorkspace === 'sessions'" class="session-workbench">
    <header class="session-command-bar">
      <SessionTopbar />
      <SessionToolbar />
    </header>

    <div :class="['session-page', { 'diagnostics-collapsed': diagnosticsCollapsed }]">
      <aside class="session-rail">
        <SessionTabs />
      </aside>

      <section class="session-workspace">
        <SessionViewportGrid />
      </section>

      <aside v-if="!diagnosticsCollapsed" class="session-inspector">
        <SessionDiagnosticsPanel />
      </aside>
    </div>
  </section>
</template>
