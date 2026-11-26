<script setup lang="ts">
import { watch } from "vue";
import { useMessage } from "naive-ui";
import { useVncAppContext } from "../composables/useVncAppContext.ts";

const { appMessageEvent } = useVncAppContext();
const message = useMessage();

watch(appMessageEvent, (event) => {
  if (!event) {
    return;
  }

  message[event.tone](event.content, {
    duration: 2600,
    keepAliveOnHover: true,
  });
}, { immediate: true });
</script>

<template></template>
