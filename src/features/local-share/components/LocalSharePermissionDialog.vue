<script setup lang="ts">
import { computed } from "vue";
import { NButton, NModal, NSpace } from "naive-ui";
import type { Translate } from "../../../app/types.ts";
import type { LocalVncServerPermissionKind } from "../../../server/localShare.ts";
import type { LocalShareStartPhase } from "../composables/useLocalShareStartFlow.ts";

const props = defineProps<{
  show: boolean;
  phase: LocalShareStartPhase;
  purpose: "start" | "permission" | "repair";
  required: LocalVncServerPermissionKind[];
  missing: LocalVncServerPermissionKind[];
  signatureChanged: LocalVncServerPermissionKind[];
  busy: boolean;
  rechecking?: boolean;
  canCancel: boolean;
  t: Translate;
}>();
defineEmits<{ cancel: []; request: []; recheck: []; openSettings: [] }>();
const currentPermission = computed(() => props.missing[0]);
const signatureChanged = computed(() => !!currentPermission.value && props.signatureChanged.includes(currentPermission.value));
</script>

<template>
  <NModal
    :show="show"
    preset="card"
    :title="currentPermission ? t('localShare.permissions.requestNamed', { permission: t(`localShare.permissions.kind.${currentPermission}`) }) : t('localShare.permissions.guideTitle')"
    size="small"
    :bordered="false"
    :closable="false"
    :mask-closable="false"
    :close-on-esc="false"
    class="local-share-permission-dialog"
    style="width: min(420px, calc(100vw - 32px))"
    :header-style="{ padding: '14px 18px 10px', fontSize: '16px' }"
    :content-style="{ padding: '0 18px 12px' }"
    :footer-style="{ padding: '0 18px 16px' }"
  >
    <template v-if="currentPermission">
      <ol v-if="required.length > 1" class="permission-steps" :aria-label="t('localShare.permissions.stepsLabel')">
        <li v-for="(kind, index) in required" :key="kind" :class="{ complete: !missing.includes(kind), current: currentPermission === kind }" :aria-current="currentPermission === kind ? 'step' : undefined">
          <span>{{ !missing.includes(kind) ? '✓' : index + 1 }}</span>
          {{ t(`localShare.permissions.kind.${kind}`) }}
        </li>
      </ol>
      <p>{{ t(`localShare.permissions.instructions.${currentPermission}`) }}</p>
      <p v-if="signatureChanged" class="permission-note">{{ t('localShare.permissions.signatureChanged') }}</p>
      <p v-if="phase === 'recovering'" class="permission-note" role="status">{{ t('localShare.permissions.startPhases.recovering') }}</p>
      <p class="permission-note">{{ t('localShare.permissions.requestNotice', { permission: t(`localShare.permissions.kind.${currentPermission}`) }) }}</p>
      <p class="permission-note">{{ t(purpose === 'start' ? 'localShare.permissions.returnToStart' : 'localShare.permissions.returnToCheck') }}</p>
      <NButton class="permission-settings-link" text type="primary" size="small" :disabled="busy || rechecking || phase !== 'awaiting_permission'" @click="$emit('openSettings')">{{ t('localShare.permissions.openSettings') }}</NButton>
    </template>
    <template #footer>
      <NSpace justify="end" :size="8">
        <NButton v-if="canCancel" size="small" @click="$emit('cancel')">{{ t(purpose === 'start' ? 'localShare.permissions.cancelStart' : 'localShare.permissions.cancel') }}</NButton>
        <NButton size="small" :loading="rechecking" :disabled="busy || rechecking || phase !== 'awaiting_permission'" @click="$emit('recheck')">{{ t('localShare.permissions.recheck') }}</NButton>
        <NButton size="small" type="primary" :loading="phase === 'requesting_permission'" :disabled="busy || rechecking || phase !== 'awaiting_permission'" @click="$emit('request')">{{ t('localShare.permissions.requestPrompt') }}</NButton>
      </NSpace>
    </template>
  </NModal>
</template>

<style scoped>
.permission-steps { display: flex; flex-wrap: wrap; gap: 16px; padding: 0; margin: 0 0 14px; list-style: none; font-size: 12px; }
.permission-steps li { display: flex; align-items: center; gap: 6px; opacity: 0.55; }
.permission-steps li.current, .permission-steps li.complete { opacity: 1; }
.permission-steps li.current { font-weight: 600; }
.permission-steps li span { display: grid; place-items: center; width: 20px; height: 20px; border: 1px solid currentColor; border-radius: 50%; }
.permission-note { opacity: 0.75; font-size: 12px; }
.permission-settings-link { margin-top: 10px; }
p { margin: 8px 0 0; font-size: 13px; line-height: 1.55; }
</style>
