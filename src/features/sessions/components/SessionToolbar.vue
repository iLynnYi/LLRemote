<script setup lang="ts">
import { NButton } from "naive-ui";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";

const {
  diagnosticsCollapsed,
  openTemporaryConnectionConfirm,
  t,
} = useVncAppContext();
</script>

<template>
  <div class="session-toolbar">
    <NButton @click="diagnosticsCollapsed = !diagnosticsCollapsed">
      {{ diagnosticsCollapsed ? t("sessions.showDiagnostics") : t("sessions.hideDiagnostics") }}
    </NButton>
    <NButton type="primary" @click="openTemporaryConnectionConfirm">{{ t("sessions.newConnection") }}</NButton>
  </div>
</template>
