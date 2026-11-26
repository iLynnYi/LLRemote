<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import { NButton, NDropdown, NInput } from "naive-ui";
import type { DropdownOption } from "naive-ui";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";
import HostGroupCreatePopover from "./HostGroupCreatePopover.vue";
import HostGroupPanelToggle from "./HostGroupPanelToggle.vue";

const {
  cancelGroupEdit, canSaveGroupRename, editingGroupName, groupRenameDraft,
  groupValidationMessage, HOST_GROUP_NAME_MAX_LENGTH, hostGroupOptions,
  hostGroupSearchQuery, hostSummary, renameSelectedGroup, selectedGroup,
  selectHostGroup, selectAllHosts, visibleHostGroups, canManageHostGroup, manageHostGroup, t,
} = useVncAppContext();

const contextGroup = ref<string | null>(null);
const panelElement = ref<HTMLElement | null>(null);
const menuX = ref(0);
const menuY = ref(0);
const contextMenuOptions = computed(() => groupMenuOptions(contextGroup.value ?? ""));

function groupMenuOptions(group: string): DropdownOption[] {
  return [
    { label: t("hosts.rename"), key: "rename", disabled: !canManageHostGroup(group) },
    { label: t("hosts.delete"), key: "delete", disabled: !canManageHostGroup(group) },
  ];
}

async function openGroupMenu(group: string, x: number, y: number) {
  contextGroup.value = null;
  await nextTick();
  menuX.value = x;
  menuY.value = y;
  contextGroup.value = group;
}

function closeGroupMenu() {
  contextGroup.value = null;
}

async function selectGroupMenuAction(key: string | number) {
  const group = contextGroup.value;
  closeGroupMenu();
  if (!group) return;
  if (key === "rename") await renameGroup(group);
  else await manageHostGroup(String(key), group);
}

async function renameGroup(group: string) {
  closeGroupMenu();
  await manageHostGroup("rename", group);
  if (editingGroupName.value === group) {
    await nextTick();
    panelElement.value?.querySelector<HTMLInputElement>(".host-group-edit input")?.focus();
  }
}

function handleGroupKeydown(event: KeyboardEvent, group: string) {
  if (event.key === "F2") {
    event.preventDefault();
    void renameGroup(group);
  } else if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
    event.preventDefault();
    const bounds = (event.currentTarget as HTMLElement).getBoundingClientRect();
    void openGroupMenu(group, bounds.left + 12, bounds.bottom);
  }
}
</script>

<template>
  <aside ref="panelElement" id="host-group-panel" class="host-group-panel" :aria-label="t('hosts.group')">
    <div class="host-group-panel-heading">
      <div class="host-group-panel-title">
        <HostGroupPanelToggle />
        <strong>{{ t("hosts.group") }} <small>{{ hostGroupOptions.length }}</small></strong>
      </div>
      <HostGroupCreatePopover />
    </div>
    <NInput
      v-model:value="hostGroupSearchQuery"
      size="small"
      :placeholder="t('hosts.searchGroups')"
      :input-props="{ 'aria-label': t('hosts.searchGroups') }"
      autocomplete="off"
      clearable
    />
    <nav class="host-group-list" :aria-label="t('hosts.group')" @scroll.passive="closeGroupMenu">
      <NButton
        quaternary
        :class="['host-group-choice', 'host-group-all', { active: !selectedGroup }]"
        :aria-pressed="!selectedGroup"
        @click="selectAllHosts"
    >
        <span>{{ t("hosts.views.all") }}</span><strong>{{ hostSummary.total }}</strong>
      </NButton>
      <template v-for="group in visibleHostGroups" :key="group.name">
        <form v-if="editingGroupName === group.name" class="host-group-edit" @submit.prevent="renameSelectedGroup">
          <NInput
            v-model:value="groupRenameDraft"
            autofocus
            size="small"
            :maxlength="HOST_GROUP_NAME_MAX_LENGTH"
            :input-props="{ 'aria-label': t('hosts.groupName') }"
            :status="groupValidationMessage ? 'error' : undefined"
            autocomplete="off"
            @keydown.escape.prevent="cancelGroupEdit"
          />
          <div class="host-group-edit-actions">
            <NButton size="tiny" :disabled="!canSaveGroupRename" attr-type="submit">{{ t("hosts.save") }}</NButton>
            <NButton size="tiny" quaternary @click="cancelGroupEdit">{{ t("hosts.cancel") }}</NButton>
          </div>
          <small v-if="groupValidationMessage" class="host-group-error" role="alert">{{ groupValidationMessage }}</small>
        </form>
        <div v-else :class="['host-group-row', { active: selectedGroup === group.name }]">
          <NButton
            quaternary
            class="host-group-choice"
            :aria-pressed="selectedGroup === group.name"
            :title="group.name"
            @click="selectHostGroup(group.name)"
            @dblclick.stop="renameGroup(group.name)"
            @contextmenu.prevent="openGroupMenu(group.name, $event.clientX, $event.clientY)"
            @keydown="handleGroupKeydown($event, group.name)"
          >
            <span>{{ group.name }}</span><strong>{{ group.count }}</strong>
          </NButton>
        </div>
      </template>
      <p v-if="!visibleHostGroups.length" class="host-group-empty">{{ t("hosts.noMatchingGroups") }}</p>
    </nav>
    <NDropdown
        trigger="manual"
        placement="bottom-start"
        :show="Boolean(contextGroup)"
        :x="menuX"
        :y="menuY"
        :options="contextMenuOptions"
        @clickoutside="closeGroupMenu"
        @update:show="(show) => !show && closeGroupMenu()"
        @select="selectGroupMenuAction"
    />
  </aside>
</template>
