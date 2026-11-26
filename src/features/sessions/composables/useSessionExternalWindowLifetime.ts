/** Owns window listeners, including registrations that finish after unmount. */
export function useSessionExternalWindowLifetime() {
  let active = true;
  const cleanups = new Set<() => void>();

  async function register(
    registerListener: () => Promise<() => void>,
    onReady?: () => void,
  ): Promise<void> {
    if (!active) return;
    const cleanup = await registerListener();
    if (!active) {
      cleanup();
      return;
    }
    cleanups.add(cleanup);
    onReady?.();
  }

  function dispose() {
    if (!active) return;
    active = false;
    const ownedCleanups = [...cleanups];
    cleanups.clear();
    for (const cleanup of ownedCleanups) cleanup();
  }

  return { isActive: () => active, register, dispose };
}
