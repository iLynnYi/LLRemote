<script setup lang="ts">
import { computed } from "vue";
import { useVncAppContext } from "../../../app/composables/useVncAppContext.ts";
import type { RemoteViewportTone } from "../../../session/viewport/remoteViewport.ts";
import type { RemoteViewportStatusHudSummary } from "../../../session/viewport/remoteViewportStatusBar.ts";
import type { RemoteSessionTab } from "../../../session/core/sessionManager.ts";
import type { SessionToolboxContext } from "../../../session/transfers/sessionToolbox.ts";
import SessionToolbox from "./SessionToolbox.vue";
import { useSessionViewportNavigation } from "../composables/useSessionViewportNavigation.ts";

const props = defineProps<{
  tab: RemoteSessionTab;
  viewportTarget: HTMLElement | null;
  isHudHidden: (sessionId: string) => boolean;
  isHudExpanded: (sessionId: string) => boolean;
  toggleHud: (sessionId: string) => void;
  toggleHudExpanded: (sessionId: string) => void;
  toggleExternalWindowFullscreen?: (sessionId: string) => Promise<void>;
  toolboxContextOverride?: SessionToolboxContext | null;
  statusHudOverride?: RemoteViewportStatusHudSummary | null;
  viewportToneOverride?: RemoteViewportTone | null;
  onEdgeScroll?: (event: PointerEvent) => void;
}>();

const {
  getRemoteViewportStatusHud,
  getRemoteViewportSummary,
  t,
} = useVncAppContext();

function viewportStateClass(state: string): string {
  return `state-${state.replace(/[^a-z0-9]+/gi, "-").replace(/^-|-$/g, "").toLowerCase()}`;
}

const statusHud = computed(() => props.statusHudOverride ?? getRemoteViewportStatusHud(props.tab));
const viewportTone = computed(() => props.viewportToneOverride ?? getRemoteViewportSummary(props.tab).tone);
const desktopNotice = computed(() => props.tab.state === "connected" && statusHud.value.runtimeDesktop
  && statusHud.value.runtimeDesktop !== "available" ? statusHud.value.runtimeDesktop : null);

useSessionViewportNavigation({
  viewport: computed(() => props.viewportTarget),
  enabled: computed(() => props.tab.state === "connected" && !desktopNotice.value
    && (props.tab.active || Boolean(props.toolboxContextOverride?.externalWindow))),
  onScroll: event => props.onEdgeScroll?.(event),
});
</script>

<template>
  <div v-if="desktopNotice" class="remote-desktop-notice" role="status" aria-live="polite">
    <div>{{ t(`sessions.runtimeDesktop.${desktopNotice}`) }}</div>
  </div>
  <div class="remote-viewport-floating-stack">
    <button
      v-if="!props.isHudHidden(tab.id)"
      type="button"
      :class="[
        'remote-viewport-hud',
        props.isHudExpanded(tab.id) ? 'expanded' : 'collapsed',
        `tone-${viewportTone}`,
      ]"
      :title="props.isHudExpanded(tab.id) ? t('sessions.hideViewportHud') : t('sessions.showViewportHud')"
      :aria-expanded="props.isHudExpanded(tab.id)"
      @click="props.toggleHudExpanded(tab.id)"
    >
      <span class="remote-viewport-hud-icon" aria-hidden="true">
        <span class="remote-viewport-hud-progress"></span>
        <span :class="['remote-viewport-hud-badge', viewportStateClass(statusHud.state)]"></span>
      </span>
      <template v-if="props.isHudExpanded(tab.id)">
        <header class="remote-viewport-hud-heading">
          <strong :title="statusHud.title">{{ statusHud.title }}</strong>
          <span :title="statusHud.target">{{ statusHud.target }}</span>
        </header>
        <dl class="remote-viewport-hud-metrics">
          <div>
            <dt>{{ t("sessions.statusHud.state") }}</dt>
            <dd :class="['remote-viewport-state', viewportStateClass(statusHud.state)]" :title="statusHud.state">
              {{ statusHud.state }}
            </dd>
          </div>
          <div>
            <dt>{{ t("sessions.statusHud.duration") }}</dt>
            <dd :title="statusHud.duration">{{ statusHud.duration }}</dd>
          </div>
          <div>
            <dt>{{ t("sessions.statusHud.input") }}</dt>
            <dd :title="t(`sessions.inputModes.${statusHud.inputMode}`)">{{ t(`sessions.inputModes.${statusHud.inputMode}`) }}</dd>
          </div>
          <div>
            <dt>{{ t("sessions.statusHud.display") }}</dt>
            <dd :title="statusHud.display">{{ statusHud.display }}</dd>
          </div>
        </dl>
      </template>
    </button>
  </div>
  <SessionToolbox
    :session-id="tab.id"
    :viewport-target="viewportTarget"
    :is-hud-hidden="props.isHudHidden"
    :toggle-hud="props.toggleHud"
    :toggle-external-window-fullscreen="props.toggleExternalWindowFullscreen"
    :context-override="toolboxContextOverride"
  />
</template>

<style scoped>
.remote-desktop-notice {
  position: absolute;
  inset: 0;
  z-index: 4;
  display: grid;
  place-items: center;
  padding: 24px;
  pointer-events: none;
  background: color-mix(in srgb, var(--surface-color, #18181c) 65%, transparent);
}
.remote-desktop-notice > div {
  max-width: 440px;
  padding: 16px 20px;
  border-radius: 8px;
  background: var(--surface-color, #18181c);
  color: var(--text-color, #eee);
  line-height: 1.7;
}
</style>
