<script setup lang="ts">
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";

const {
  t,
} = useVncAppContext();
</script>

<template>
  <div class="session-topbar">
    <h2>{{ t("sessions.title") }}</h2>
  </div>
</template>
