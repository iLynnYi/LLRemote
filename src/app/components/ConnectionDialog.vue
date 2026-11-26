<script setup lang="ts">
import { computed } from "vue";
import { NButton, NCheckbox, NInput, NInputNumber, NModal, NRadio, NRadioGroup, NSpace, NSwitch, useThemeVars } from "naive-ui";
import { useVncAppContext } from "../composables/useVncAppContext.ts";
import type { PostConnectPresentation } from "../../session/core/connectionConfirm.ts";
import { createPortInputProps } from "../utils/portInput.ts";

const {
  closeConnectionConfirm,
  connectionConfirmDraft,
  submitConnectionConfirm,
  t,
  updateConnectionCredentialStatus,
} = useVncAppContext();

const postConnectPresentationOptions: PostConnectPresentation[] = ["main", "fullscreen", "external"];
const portInputProps = createPortInputProps();
const theme = useThemeVars();
const dialogStyle = computed(() => ({
  "--connection-text-color": theme.value.textColor1,
  "--connection-muted-color": theme.value.textColor3,
  "--connection-border-color": theme.value.borderColor,
  "--connection-surface-color": theme.value.cardColor,
  "--connection-primary-color": theme.value.primaryColor,
}));
</script>

<template>
<NModal
  :show="Boolean(connectionConfirmDraft)"
  preset="card"
  class="connection-dialog"
  :style="dialogStyle"
  :title="t(connectionConfirmDraft?.source === 'temporary' ? 'sessions.newConnection' : 'sessions.confirmTitle')"
  :bordered="false"
  :closable="false"
  :mask-closable="false"
  @esc="closeConnectionConfirm"
>
  <template v-if="connectionConfirmDraft">
    <div v-if="connectionConfirmDraft.source === 'saved-host'" class="connection-dialog-title">
      <strong>{{ connectionConfirmDraft.name }}</strong>
      <span>{{ t("sessions.sources.saved-host") }}</span>
    </div>

    <form class="connection-form" @submit.prevent="submitConnectionConfirm">
      <section class="connection-section">
        <div class="connection-host-port-row">
          <label class="connection-field">
            <span>{{ t("hosts.address") }}</span>
            <NInput v-model:value="connectionConfirmDraft.host" :readonly="connectionConfirmDraft.source === 'saved-host'" autocomplete="off" spellcheck="false" />
          </label>
          <label class="connection-field connection-port-field">
            <span>{{ t("toolbar.port") }}</span>
            <NInputNumber
              v-model:value="connectionConfirmDraft.port"
              class="connection-port-input"
              :readonly="connectionConfirmDraft.source === 'saved-host'"
              :min="1"
              :max="65535"
              :input-props="portInputProps"
            />
          </label>
        </div>

        <label class="connection-field connection-password-field">
          <span>{{ t("toolbar.password") }}</span>
          <NInput
            v-model:value="connectionConfirmDraft.password"
            type="password"
            autocomplete="off"
            :placeholder="t('sessions.passwordPlaceholder')"
            @update:value="updateConnectionCredentialStatus"
          />
        </label>
      </section>

      <div v-if="connectionConfirmDraft.source === 'temporary'" class="connection-save-host">
        <label class="connection-save-toggle">
          <span>{{ t("sessions.saveTemporaryHost") }}</span>
          <NSwitch
            v-model:value="connectionConfirmDraft.options.saveTemporaryHost"
            :aria-label="t('sessions.saveTemporaryHost')"
          />
        </label>
        <label v-if="connectionConfirmDraft.options.saveTemporaryHost" class="connection-save-name">
          <span>{{ t("hosts.name") }}</span>
          <NInput
            v-model:value="connectionConfirmDraft.name"
            :placeholder="`${connectionConfirmDraft.host}:${connectionConfirmDraft.port}`"
            autocomplete="off"
          />
        </label>
      </div>

      <details class="connection-more-options">
        <summary>{{ t("sessions.moreOptions") }}</summary>
        <div class="connection-options">
          <NCheckbox v-model:checked="connectionConfirmDraft.options.readonly">
            {{ t("sessions.readonly") }}
          </NCheckbox>
          <div class="connection-option-choice">
            <span class="connection-option-label">{{ t("sessions.postConnectPresentation") }}</span>
            <NRadioGroup v-model:value="connectionConfirmDraft.options.postConnectPresentation" class="connection-radio-group">
              <NRadio
                v-for="option in postConnectPresentationOptions"
                :key="option"
                :value="option"
              >
                {{ t(`sessions.postConnectPresentations.${option}`) }}
              </NRadio>
            </NRadioGroup>
          </div>
        </div>
      </details>

      <NSpace class="confirm-actions" justify="end">
        <NButton quaternary @click="closeConnectionConfirm">{{ t("hosts.cancel") }}</NButton>
        <NButton attr-type="submit" type="primary">{{ t("toolbar.connect") }}</NButton>
      </NSpace>
    </form>
  </template>
</NModal>
</template>
