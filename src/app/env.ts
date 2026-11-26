export function isEnvFlagEnabled(value: unknown): boolean {
  if (typeof value !== "string") {
    return false;
  }

  return ["1", "true", "yes", "on"].includes(value.trim().toLowerCase());
}

export function showDeveloperDebugInfo(): boolean {
  return import.meta.env.DEV && isEnvFlagEnabled(import.meta.env.VITE_SHOW_DEBUG_INFO);
}
