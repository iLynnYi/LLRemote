import type { LocalVncServerOverview } from "../../server/localShare.ts";
import type { AgentDisplayInventoryResult } from "../protocol/protocolAdapter.ts";

export type SessionDisplayChoice = {
  value: string;
  label: string;
  selected: boolean;
  remote: boolean;
  isPrimary: boolean;
  width: number;
  height: number;
};

export type LocalServerDisplayInventory = Pick<LocalVncServerOverview, "displays" | "selectedDisplayId">;

/** Serializable display choices shared by the main and external toolboxes. */
export function createSessionDisplayChoices(
  localServer: LocalServerDisplayInventory | null | undefined,
  remoteInventory: AgentDisplayInventoryResult | null | undefined,
  selectedRemoteDisplayId: string | null | undefined,
): SessionDisplayChoice[] {
  if (localServer?.displays.length) {
    return localServer.displays.map(display => ({
      value: display.id, label: display.label, selected: display.id === localServer.selectedDisplayId,
      remote: false, isPrimary: display.isPrimary, width: display.width, height: display.height,
    }));
  }
  const displays = remoteInventory?.displays ?? [];
  const selectedId = displays.find(display => display.id === selectedRemoteDisplayId)?.id
    ?? displays.find(display => display.isPrimary)?.id ?? displays[0]?.id;
  return displays.map(display => ({
    value: `remote:${display.id}`, label: display.label, selected: display.id === selectedId,
    remote: true, isPrimary: display.isPrimary, width: display.width, height: display.height,
  }));
}
