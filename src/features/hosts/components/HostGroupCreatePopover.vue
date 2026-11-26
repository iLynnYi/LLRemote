<script setup lang="ts">
import { nextTick, ref } from "vue";
import { NButton, NIcon, NInput, NPopover, useThemeVars } from "naive-ui";
import type { InputInst } from "naive-ui";
import { AddOutline } from "@vicons/ionicons5";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";

const {
  canCreateGroup,
  createHostGroup,
  groupValidationMessage,
  HOST_GROUP_NAME_MAX_LENGTH,
  newGroupDraft,
  t,
} = useVncAppContext();
const theme = useThemeVars();
const visible = ref(false);
const creating = ref(false);
const input = ref<InputInst | null>(null);
const trigger = ref<HTMLButtonElement | null>(null);

async function updateVisible(show: boolean) {
  if (creating.value) return;
  newGroupDraft.value = "";
  groupValidationMessage.value = "";
  visible.value = show;
  if (show) {
    await nextTick();
    input.value?.focus();
  }
}

function cancel() {
  if (creating.value) return;
  void updateVisible(false);
  trigger.value?.focus();
}

async function submit() {
  if (creating.value) return;
  creating.value = true;
  let created = false;
  try {
    created = await createHostGroup();
  } finally {
    creating.value = false;
  }
  if (created) {
    visible.value = false;
    await nextTick();
    trigger.value?.focus();
  }
}
</script>

<template>
  <NPopover :show="visible" trigger="click" placement="right-start" @update:show="updateVisible">
    <template #trigger>
      <button
        ref="trigger"
        type="button"
        class="group-create-trigger"
        :title="t('hosts.createGroup')"
        :aria-label="t('hosts.createGroup')"
        aria-haspopup="dialog"
        :aria-expanded="visible"
        :disabled="creating"
      >
        <NIcon :size="18"><AddOutline /></NIcon>
      </button>
    </template>
    <form
      class="group-create-form"
      role="dialog"
      :aria-label="t('hosts.createGroup')"
      @submit.prevent="submit"
      @keydown.escape.stop.prevent="cancel"
    >
      <h3>{{ t("hosts.createGroup") }}</h3>
      <NInput
        ref="input"
        v-model:value="newGroupDraft"
        :maxlength="HOST_GROUP_NAME_MAX_LENGTH"
        :placeholder="t('hosts.newGroupPlaceholder')"
        :input-props="{ 'aria-label': t('hosts.groupName'), 'aria-describedby': 'group-create-feedback' }"
        :disabled="creating"
        :status="groupValidationMessage ? 'error' : undefined"
        autocomplete="off"
        @update:value="groupValidationMessage = ''"
      />
      <p v-if="groupValidationMessage" id="group-create-feedback" class="group-create-error" role="alert">{{ groupValidationMessage }}</p>
      <p v-else id="group-create-feedback" class="group-create-hint">{{ t("hosts.groupNameLength") }}</p>
      <div class="group-create-actions">
        <NButton size="small" :disabled="creating" @click="cancel">{{ t("hosts.cancel") }}</NButton>
        <NButton size="small" attr-type="submit" type="primary" :loading="creating" :disabled="!canCreateGroup">{{ t("hosts.create") }}</NButton>
      </div>
    </form>
  </NPopover>
</template>

<style scoped>
.group-create-trigger {
  display: grid;
  place-items: center;
  flex: 0 0 auto;
  width: 28px;
  height: 28px;
  padding: 0;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: v-bind('theme.textColor3');
  cursor: pointer;
}

.group-create-trigger:hover {
  background: v-bind('theme.hoverColor');
  color: v-bind('theme.primaryColor');
}

.group-create-trigger:focus-visible {
  outline: 2px solid v-bind('theme.primaryColor');
  outline-offset: 2px;
}

.group-create-trigger:disabled {
  cursor: default;
  opacity: 0.5;
}

.group-create-form {
  width: 248px;
}

.group-create-form h3 {
  margin: 0 0 12px;
  color: v-bind('theme.textColor1');
  font-size: 14px;
}

.group-create-hint,
.group-create-error {
  margin: 7px 0 14px;
  font-size: 12px;
  line-height: 1.5;
}

.group-create-hint {
  color: v-bind('theme.textColor3');
}

.group-create-error {
  color: v-bind('theme.errorColor');
}

.group-create-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
