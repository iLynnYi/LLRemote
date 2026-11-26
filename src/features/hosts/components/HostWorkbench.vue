<script setup lang="ts">
import { computed } from "vue";
import { NButton, NDropdown, NEmpty, NIcon, NInput, useThemeVars } from "naive-ui";
import type { DropdownOption } from "naive-ui";
import { DesktopOutline } from "@vicons/ionicons5";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";
import type { HostRecord } from "../../../hosts/hostManager.ts";
import HostEditorDialog from "./HostEditorDialog.vue";
import HostGroupSidebar from "./HostGroupSidebar.vue";
import HostGroupPanelToggle from "./HostGroupPanelToggle.vue";

const {
  activeHostScopeLabel,
  activeWorkspace,
  connectSelectedHost,
  connectSelectedHostRecord,
  deleteHostRecord,
  fillSelectedHostConnectionForm,
  filteredHosts,
  formatHostDateTime,
  hostSearchQuery,
  hostGroupsCollapsed,
  openHostCreateForm,
  openHostEditForm,
  selectedHost,
  selectHostRecord,
  t,
} = useVncAppContext();

const theme = useThemeVars();
const cardTheme = computed(() => ({
  "--host-card-color": theme.value.cardColor,
  "--host-text-color": theme.value.textColor1,
  "--host-muted-color": theme.value.textColor3,
  "--host-border-color": theme.value.borderColor,
  "--host-primary-color": theme.value.primaryColor,
}));

function hostCardMenuOptions(): DropdownOption[] {
  return [
    { label: t("hosts.use"), key: "fill" },
    { label: t("hosts.edit"), key: "edit" },
    { label: t("toolbar.connect"), key: "connect" },
    { type: "divider", key: "host-card-danger-divider" },
    { label: t("hosts.delete"), key: "delete" },
  ];
}

async function handleHostCardMenuSelect(key: string | number, hostRecord: HostRecord) {
  await selectHostRecord(hostRecord);

  if (key === "fill") {
    await fillSelectedHostConnectionForm(hostRecord);
    return;
  }

  if (key === "edit") {
    openHostEditForm(hostRecord);
    return;
  }

  if (key === "connect") {
    await connectSelectedHost();
    return;
  }

  if (key === "delete") {
    deleteHostRecord(hostRecord);
  }
}

async function connectHostRecordQuickly(hostRecord: HostRecord) {
  await selectHostRecord(hostRecord);
  await connectSelectedHostRecord(hostRecord);
}

</script>

<template>
<section v-show="activeWorkspace === 'hosts'" class="host-workbench" :style="cardTheme">
  <div class="host-topbar">
    <div class="host-page-heading">
      <h2>{{ t("modules.hosts") }}</h2>
    </div>
    <div class="host-actions">
      <NInput v-model:value="hostSearchQuery" :placeholder="t('hosts.search')" autocomplete="off" spellcheck="false" clearable />
      <NButton type="primary" @click="openHostCreateForm">{{ t("hosts.add") }}</NButton>
    </div>
  </div>
  <div :class="['host-workspace', { 'groups-collapsed': hostGroupsCollapsed }]">
    <HostGroupSidebar v-show="!hostGroupsCollapsed" />
    <section class="host-catalog">
      <div class="host-list-header">
        <div class="host-list-scope">
          <HostGroupPanelToggle v-if="hostGroupsCollapsed" collapsed />
          <span>{{ activeHostScopeLabel }} <strong>{{ filteredHosts.length }}</strong></span>
        </div>
        <small>{{ t("hosts.doubleClickToConnect") }}</small>
      </div>
      <div v-if="filteredHosts.length" class="host-card-grid">
        <article
          v-for="hostRecord in filteredHosts"
          :key="hostRecord.id"
          :class="['host-card', { active: selectedHost?.id === hostRecord.id }]"
          @dblclick.stop.prevent="connectHostRecordQuickly(hostRecord)"
        >
          <button
            type="button"
            class="host-card-main"
            :aria-label="hostRecord.name + ' · ' + hostRecord.target"
            :aria-pressed="selectedHost?.id === hostRecord.id"
            @click="selectHostRecord(hostRecord)"
            @keyup.enter="connectHostRecordQuickly(hostRecord)"
          >
            <span class="host-card-heading">
              <NIcon :size="20"><DesktopOutline /></NIcon>
              <strong :title="hostRecord.name">{{ hostRecord.name }}</strong>
            </span>
            <span class="host-card-target" :title="hostRecord.target">{{ hostRecord.target }}</span>
            <span class="host-card-meta">
              <span :title="hostRecord.group">{{ hostRecord.group }}</span>
              <span>{{ t(hostRecord.credentialRef ? "hosts.authMethodPassword" : "hosts.authMethodPrompt") }}</span>
            </span>
            <span class="host-card-history">
              {{ t("hosts.lastConnected") }} · {{ formatHostDateTime(hostRecord.lastConnectedAt) }}
            </span>
          </button>
          <div class="host-card-actions" @click.stop @dblclick.stop>
            <NDropdown
              trigger="click"
              placement="bottom-end"
              :options="hostCardMenuOptions()"
              @select="(key) => handleHostCardMenuSelect(key, hostRecord)"
            >
              <NButton
                size="small"
                quaternary
                circle
                class="host-more-action"
                :title="t('hosts.moreActions')"
                :aria-label="t('hosts.moreActions')"
              >
                ...
              </NButton>
            </NDropdown>
          </div>
        </article>
      </div>
      <NEmpty v-else class="host-empty" :description="t('hosts.emptyDescription')">
        <template #extra>
          <strong>{{ t("hosts.emptyTitle") }}</strong>
        </template>
      </NEmpty>
    </section>
  </div>
  <HostEditorDialog />
</section>
</template>
