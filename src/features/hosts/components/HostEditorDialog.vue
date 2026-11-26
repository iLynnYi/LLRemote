<script setup lang="ts">
import { computed } from "vue";
import { NButton, NInput, NInputNumber, NModal, NSelect, useThemeVars } from "naive-ui";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";
import { createPortInputProps } from "../../../app/utils/portInput.ts";

const {
  closeHostEditor,
  hostCreateVisible,
  hostCredentialMethod,
  hostCredentialPlaceholder,
  hostDraft,
  hostEditorMode,
  hostGroupOptions,
  hostSavePending,
  saveHost,
  t,
} = useVncAppContext();

const groupOptions = computed(() => hostGroupOptions.value.map((group) => ({ label: group.name, value: group.name })));
const credentialOptions = computed(() => [
  { label: t("hosts.authMethodPrompt"), value: "prompt" },
  { label: t("hosts.authMethodPassword"), value: "password" },
]);
const theme = useThemeVars();
const portInputProps = createPortInputProps();
</script>

<template>
  <NModal
    :show="hostCreateVisible"
    preset="card"
    class="host-editor-dialog"
    :style="{ '--host-editor-muted-color': theme.textColor3 }"
    :title="t(hostEditorMode === 'edit' ? 'hosts.editTitle' : 'hosts.createTitle')"
    :bordered="false"
    :mask-closable="false"
    :closable="!hostSavePending"
    :close-on-esc="!hostSavePending"
    @update:show="(show) => !show && closeHostEditor()"
  >
    <form id="host-editor-form" class="host-form" @submit.prevent="saveHost">
      <label class="host-form-wide">
        {{ t("hosts.name") }}
        <NInput v-model:value="hostDraft.name" :disabled="hostSavePending" autocomplete="off" />
      </label>
      <div class="host-address-port-row host-form-wide">
        <label>
          {{ t("hosts.address") }}
          <NInput v-model:value="hostDraft.address" :disabled="hostSavePending" autocomplete="off" spellcheck="false" />
        </label>
        <label>
          {{ t("toolbar.port") }}
          <NInputNumber v-model:value="hostDraft.port" :disabled="hostSavePending" :min="1" :max="65535" :input-props="portInputProps" />
        </label>
      </div>
      <label>
        {{ t("hosts.group") }}
        <NSelect v-model:value="hostDraft.group" :disabled="hostSavePending" :options="groupOptions" />
      </label>
      <label>
        {{ t("hosts.authMethod") }}
        <NSelect v-model:value="hostCredentialMethod" :disabled="hostSavePending" :options="credentialOptions" />
      </label>
      <label v-if="hostCredentialMethod === 'password'">
        {{ t("toolbar.password") }}
        <NInput
          v-model:value="hostDraft.credential.password"
          :disabled="hostSavePending"
          type="password"
          autocomplete="off"
          :placeholder="hostCredentialPlaceholder"
        />
        <small class="host-password-hint">{{ t('hosts.passwordStorageHint') }}</small>
      </label>
    </form>
    <template #footer>
      <div class="host-editor-actions">
        <NButton :disabled="hostSavePending" @click="closeHostEditor">{{ t("hosts.cancel") }}</NButton>
        <NButton attr-type="submit" form="host-editor-form" type="primary" :loading="hostSavePending">{{ t("hosts.save") }}</NButton>
      </div>
    </template>
  </NModal>
</template>
