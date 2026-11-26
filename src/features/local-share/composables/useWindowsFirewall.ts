import { computed, onBeforeUnmount, ref, type Ref } from "vue";
import type { Translate } from "../../../app/types.ts";
import { createWindowsFirewallApi, WINDOWS_FIREWALL_NETWORKS, windowsFirewallErrorKey, type WindowsFirewallOverview } from "../../../server/windowsFirewall.ts";

export function useWindowsFirewall(options: {
  active: Readonly<Ref<boolean>>;
  invoke: Parameters<typeof createWindowsFirewallApi>[0];
  t: Translate;
}) {
  const api = createWindowsFirewallApi(options.invoke);
  const overview = ref<WindowsFirewallOverview | null>(null);
  const checking = ref(false);
  const openingSettings = ref(false);
  const error = ref("");
  const settingsError = ref("");
  const profiles = computed(() => WINDOWS_FIREWALL_NETWORKS.flatMap(name => {
    const profile = overview.value?.profiles.find(profile => profile.name === name);
    return profile ? [profile] : [];
  }));
  let alive = true;
  let reading: Promise<void> | null = null;
  let settingsReturnPending = false;
  let settingsDepartureSeen = false;

  function refresh(): Promise<void> {
    if (!alive || !options.active.value) return Promise.resolve();
    if (reading) return reading;
    checking.value = true;
    error.value = "";
    reading = (async () => {
      try {
        const result = await api.inspect();
        if (alive) overview.value = result;
      } catch (cause) {
        if (alive) {
          overview.value = null;
          error.value = options.t(windowsFirewallErrorKey(cause));
        }
      } finally {
        checking.value = false;
        reading = null;
      }
    })();
    return reading;
  }

  async function openSettings() {
    if (!alive || openingSettings.value) return;
    openingSettings.value = true;
    settingsError.value = "";
    settingsReturnPending = true;
    settingsDepartureSeen = false;
    try { await api.openSettings(); }
    catch (cause) {
      settingsReturnPending = false;
      if (alive) settingsError.value = options.t(windowsFirewallErrorKey(cause));
    } finally { openingSettings.value = false; }
  }

  function markSettingsDeparture() {
    if (settingsReturnPending) settingsDepartureSeen = true;
  }

  function refreshAfterSettings(): Promise<void> {
    if (!settingsReturnPending || !settingsDepartureSeen || !options.active.value) return Promise.resolve();
    settingsReturnPending = false;
    settingsDepartureSeen = false;
    // A read started before the user changed settings cannot stand in for the
    // return refresh. Finish it, then query the updated system state once.
    return reading ? reading.then(() => refresh()) : refresh();
  }

  onBeforeUnmount(() => { alive = false; });
  return { profiles, checking, openingSettings, error, settingsError, refresh, openSettings, markSettingsDeparture, refreshAfterSettings };
}
