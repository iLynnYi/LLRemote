import { inject, provide, type InjectionKey } from "vue";
import type { VncAppState } from "./useVncAppState.ts";

const vncAppContextKey: InjectionKey<VncAppState> = Symbol("vnc-app-context");

export function provideVncAppContext(state: VncAppState) {
  provide(vncAppContextKey, state);
}

export function useVncAppContext(): VncAppState {
  const state = inject(vncAppContextKey);

  if (!state) {
    throw new Error("VNC app context is not available");
  }

  return state;
}
