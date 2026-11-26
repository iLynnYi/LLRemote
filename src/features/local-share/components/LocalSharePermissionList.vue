<script setup lang="ts">
import { NButton, NTag, useThemeVars } from "naive-ui";
import type { Translate } from "../../../app/types.ts";
import { localVncServerPermissionKindKey, localVncServerPermissionStatusKey, type LocalVncServerPermission } from "../../../server/localShare.ts";

defineProps<{ permissions: LocalVncServerPermission[]; busy: boolean; viewOnly: boolean; t: Translate }>();
defineEmits<{ request: [permission: LocalVncServerPermission] }>();
const theme = useThemeVars();
</script>

<template>
  <div class="permission-list">
    <div v-for="permission in permissions" :key="permission.kind" :class="['permission-row', permission.status]">
      <div class="permission-content">
        <div class="permission-heading">
          <strong>{{ t(localVncServerPermissionKindKey(permission.kind)) }}</strong>
          <NTag size="small" :bordered="false" :type="permission.status === 'granted' ? 'success' : permission.status === 'missing' ? 'warning' : 'default'">
            {{ t(localVncServerPermissionStatusKey(permission.status)) }}
          </NTag>
        </div>
        <p v-if="permission.status !== 'granted'">{{ t(permission.kind === 'accessibility' && viewOnly ? 'localShare.permissions.viewOnlyHint' : `localShare.permissions.requiredFor.${permission.requiredFor}`) }}</p>
      </div>
      <NButton v-if="permission.status === 'missing' || permission.status === 'unknown'" size="small" :disabled="busy" @click="$emit('request', permission)">
        {{ t('localShare.permissions.requestPrompt') }}
      </NButton>
    </div>
  </div>
</template>

<style scoped>
.permission-list { display: grid; gap: 10px; min-width: 0; }
.permission-row { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 12px 14px; border: 1px solid v-bind('theme.borderColor'); border-radius: 8px; background: v-bind('theme.actionColor'); }
.permission-row.granted { border-color: color-mix(in srgb, v-bind('theme.successColor') 30%, v-bind('theme.borderColor')); background: color-mix(in srgb, v-bind('theme.successColor') 7%, v-bind('theme.cardColor')); }
.permission-row.missing { border-color: color-mix(in srgb, v-bind('theme.warningColor') 40%, v-bind('theme.borderColor')); background: color-mix(in srgb, v-bind('theme.warningColor') 10%, v-bind('theme.cardColor')); }
.permission-content { min-width: 0; }
.permission-heading { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; }
.permission-heading strong { font-size: 13px; color: v-bind('theme.textColor1'); }
.permission-content p { margin: 5px 0 0; color: v-bind('theme.textColor3'); font-size: 12px; line-height: 1.5; }
.permission-row > .n-button { flex-shrink: 0; }
</style>
