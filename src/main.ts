import { createApp } from "vue";
import App from "./App.vue";
import { i18n } from "./i18n";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { prepareNativeFileTransferTaskStore } from "./session/transfers/fileTransferStore.ts";
import { isSessionExternalWindowLocation } from "./session/external-window/sessionExternalWindow.ts";

// Suppress WebView browser menus while preserving app menus and remote input.
document.addEventListener("contextmenu", (event) => {
  const target = event.target;
  if (target instanceof Element
    && target.matches("input, textarea, [contenteditable='true']")
    && target.closest("[data-native-editing]")) return;
  event.preventDefault();
}, { capture: true });

async function startApp() {
  if (isTauri() && !isSessionExternalWindowLocation(window.location.href)) {
    await prepareNativeFileTransferTaskStore(invoke);
  }
  createApp(App).use(i18n).mount("#app");
}

void startApp().catch(error => {
  console.error("app_startup_failed", error);
  if (isTauri()) void invoke("report_frontend_startup_failure");
});
