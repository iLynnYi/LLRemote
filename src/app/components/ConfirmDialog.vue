<script setup lang="ts">
import { NButton, NModal, NSpace } from "naive-ui";
import { useVncAppContext } from "../composables/useVncAppContext.ts";

const {
  confirmDialog,
  resolveConfirm,
} = useVncAppContext();
</script>

<template>
<NModal
  :show="confirmDialog.visible"
  preset="card"
  :title="confirmDialog.title"
  class="confirm-dialog"
  :bordered="false"
  :mask-closable="false"
  @update:show="(show) => !show && resolveConfirm(confirmDialog.dismissChoice ?? 'secondary')"
>
  <p>{{ confirmDialog.message }}</p>
  <template #footer>
    <NSpace justify="end">
      <NButton @click="resolveConfirm('secondary')">
        {{ confirmDialog.secondaryLabel }}
      </NButton>
      <NButton
        :type="confirmDialog.danger ? 'error' : 'primary'"
        @click="resolveConfirm('primary')"
      >
        {{ confirmDialog.primaryLabel }}
      </NButton>
    </NSpace>
  </template>
</NModal>
</template>
