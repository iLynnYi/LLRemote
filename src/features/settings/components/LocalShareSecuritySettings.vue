<script setup lang="ts">
import { ref, watch } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { NSwitch, useMessage } from "naive-ui";
import type { Translate } from "../../../app/types.ts";

const props = defineProps<{ active: boolean; t: Translate }>();
const enabled = ref<boolean | null>(null);
const busy = ref(false);
const message = useMessage();

watch(() => props.active, async active => {
  if (!active || busy.value) return;
  if (!isTauri()) { enabled.value = false; return; }
  busy.value = true;
  try {
    enabled.value = await invoke<boolean>("get_local_share_disconnect_lock");
  } catch {
    enabled.value = null;
    message.error(props.t("settings.disconnectLockLoadFailed"));
  } finally { busy.value = false; }
}, { immediate: true });

async function save(value: boolean) {
  if (busy.value || enabled.value === null || !isTauri()) return;
  busy.value = true;
  try {
    enabled.value = await invoke<boolean>("set_local_share_disconnect_lock", { enabled: value });
  } catch {
    message.error(props.t("settings.disconnectLockSaveFailed"));
  } finally { busy.value = false; }
}
</script>

<template>
  <div class="settings-group">
    <div class="settings-toggle settings-toggle-row">
      <span>
        <strong>{{ t("settings.lockAfterRemoteEnds") }}</strong>
        <small id="settings-disconnect-lock-description">{{ t("settings.lockAfterRemoteEndsDescription") }}</small>
      </span>
      <NSwitch
        :value="enabled === true"
        :loading="busy"
        :disabled="busy || enabled === null || !isTauri()"
        :aria-label="t('settings.lockAfterRemoteEnds')"
        aria-describedby="settings-disconnect-lock-description"
        @update:value="save"
      />
    </div>
  </div>
</template>
