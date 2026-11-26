export const supportedLocales = ["zh-CN"] as const;
export type SupportedLocale = (typeof supportedLocales)[number];
export const defaultLocale: SupportedLocale = "zh-CN";
