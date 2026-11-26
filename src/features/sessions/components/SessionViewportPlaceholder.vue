<script setup lang="ts">
import { darkTheme, NConfigProvider, NIcon } from "naive-ui";
import type { GlobalThemeOverrides } from "naive-ui";

const props = defineProps<{
  title: string;
  description: string;
  target?: string;
  tone?: "warning" | "info";
}>();

const themeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: "#5cc8a7",
    primaryColorHover: "#79d8bc",
    primaryColorPressed: "#42b38f",
    primaryColorSuppl: "#5cc8a7",
  },
};
</script>

<template>
  <NConfigProvider :theme="darkTheme" :theme-overrides="themeOverrides" :class="['session-placeholder-card', `tone-${props.tone ?? 'warning'}`]">
    <div class="session-placeholder-heading" role="status">
      <span class="session-placeholder-icon" aria-hidden="true"><NIcon><slot name="icon" /></NIcon></span>
      <h3>{{ props.title }}</h3>
      <span v-if="props.target" class="session-placeholder-target" :title="props.target">{{ props.target }}</span>
      <p>{{ props.description }}</p>
    </div>
    <div class="session-placeholder-buttons"><slot /></div>
  </NConfigProvider>
</template>
