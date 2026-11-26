<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";
import LocalSharePermissionDialog from "./LocalSharePermissionDialog.vue";

const {
  activeWorkspace,
  localShareStartPhase, localShareStartPending, localShareMissingPermissions,
  localShareRequiredPermissions, localShareSignatureChangedPermissions, localShareGuidePurpose,
  localVncServerLoading, cancelLocalShareStart, requestLocalShareGuidePermission,
  openLocalShareGuideSettings, resumeLocalShareStart, t,
} = useVncAppContext();
const guideRechecking = ref(false);
const active = computed(() => activeWorkspace.value === "localShare" || activeWorkspace.value === "settings");
async function recheck() {
  guideRechecking.value = true;
  try { await resumeLocalShareStart(false); }
  finally { guideRechecking.value = false; }
}
watch(activeWorkspace, workspace => {
  // The tray can bring the user to the share form to finish entering a password.
  if (workspace !== "localShare" || localShareStartPhase.value !== "awaiting_password") cancelLocalShareStart();
});
</script>

<template>
  <LocalSharePermissionDialog
    :show="active && localShareMissingPermissions.length > 0 && ['awaiting_permission', 'requesting_permission', 'recovering'].includes(localShareStartPhase)"
    :phase="localShareStartPhase"
    :purpose="localShareGuidePurpose"
    :required="localShareRequiredPermissions"
    :missing="localShareMissingPermissions"
    :signature-changed="localShareSignatureChangedPermissions"
    :busy="localVncServerLoading"
    :rechecking="guideRechecking"
    :can-cancel="localShareStartPending && localShareStartPhase !== 'starting'"
    :t="t"
    @cancel="cancelLocalShareStart"
    @request="requestLocalShareGuidePermission"
    @open-settings="openLocalShareGuideSettings"
    @recheck="recheck"
  />
</template>
