<script setup lang="ts">
import { nextTick } from "vue";
import { NButton, NIcon } from "naive-ui";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";

const props = withDefaults(defineProps<{ collapsed?: boolean }>(), { collapsed: false });
const { toggleHostGroups, t } = useVncAppContext();

async function toggle() {
  toggleHostGroups();
  await nextTick();
  document.getElementById(props.collapsed ? "hide-host-groups" : "show-host-groups")?.focus();
}
</script>

<template>
  <NButton
    :id="collapsed ? 'show-host-groups' : 'hide-host-groups'"
    class="host-groups-toggle"
    quaternary
    :aria-label="t(collapsed ? 'hosts.showGroups' : 'hosts.hideGroups')"
    :title="t(collapsed ? 'hosts.showGroups' : 'hosts.hideGroups')"
    :aria-expanded="!collapsed"
    aria-controls="host-group-panel"
    @click="toggle"
  >
    <NIcon :size="16">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
        <rect x="3" y="4" width="18" height="16" rx="2" />
        <path d="M9 4v16" />
      </svg>
    </NIcon>
  </NButton>
</template>
