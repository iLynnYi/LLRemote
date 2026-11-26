<script setup lang="ts">
import { ref } from "vue";
import { NButton, NInput, NModal, NSpace, type InputInst } from "naive-ui";
import { useVncAppContext } from "../composables/useVncAppContext.ts";

const {
  cancelPasswordPrompt,
  passwordPrompt,
  submitPasswordPrompt,
  t,
} = useVncAppContext();

const passwordInput = ref<InputInst | null>(null);
function handleSubmit() {
  if (!submitPasswordPrompt() && passwordPrompt.value.visible) passwordInput.value?.focus();
}
</script>

<template>
<NModal
  :show="passwordPrompt.visible"
  preset="card"
  class="password-dialog"
  :title="t('sessions.passwordPromptTitle')"
  :bordered="false"
  :closable="true"
  :mask-closable="false"
  :close-on-esc="true"
  @update:show="show => { if (!show) cancelPasswordPrompt(); }"
>
  <form class="password-dialog-form" @submit.prevent="handleSubmit">
    <p v-if="passwordPrompt.target" class="password-dialog-target">{{ passwordPrompt.target }}</p>

    <label>
      {{ t("toolbar.password") }}
      <NInput
        ref="passwordInput"
        v-model:value="passwordPrompt.password"
        type="password"
        show-password-on="click"
        autocomplete="current-password"
        :placeholder="t('sessions.passwordPromptPlaceholder')"
      />
    </label>

    <NSpace class="confirm-actions" justify="end">
      <NButton @click="cancelPasswordPrompt">{{ t("hosts.cancel") }}</NButton>
      <NButton attr-type="submit" type="primary">{{ t("toolbar.connect") }}</NButton>
    </NSpace>
  </form>
</NModal>
</template>
