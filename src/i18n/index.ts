import { createI18n } from "vue-i18n";
import messages from "./messages.ts";
import { defaultLocale } from "./locale.ts";

export const i18n = createI18n({
  legacy: false,
  locale: defaultLocale,
  fallbackLocale: defaultLocale,
  messages,
});

export type { SupportedLocale } from "./locale.ts";
export { useI18n } from "vue-i18n";
