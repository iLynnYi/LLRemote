<script setup lang="ts">
import {
  dateZhCN,
  NConfigProvider,
  NMessageProvider,
  zhCN,
} from "naive-ui";
import "./app/styles.css";
import AppMessageBridge from "./app/components/AppMessageBridge.vue";
import AppShell from "./app/components/AppShell.vue";
import { naiveThemeOverrides } from "./app/naiveTheme.ts";
import { provideVncAppContext } from "./app/composables/useVncAppContext.ts";
import { useVncAppState } from "./app/composables/useVncAppState.ts";
import SessionExternalWindowPage from "./features/sessions/components/SessionExternalWindowPage.vue";

const app = useVncAppState();
provideVncAppContext(app);
</script>

<template>
  <NConfigProvider
    :theme-overrides="naiveThemeOverrides"
    :locale="zhCN"
    :date-locale="dateZhCN"
  >
    <NMessageProvider placement="top">
      <AppMessageBridge />
      <SessionExternalWindowPage v-if="app.externalWindowLocation" />
      <AppShell v-else />
    </NMessageProvider>
  </NConfigProvider>
</template>
