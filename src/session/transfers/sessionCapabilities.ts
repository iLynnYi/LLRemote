import { sessionSupportsCapability, type CapabilityMatrix } from "../connection-model/connectionModel.ts";

export type SessionEnhancedCapability =
  | "message.show"
  | "clipboard.text.write"
  | "file.transfer.upload"
  | "agent.permissions.diagnose"
  | "agent.display.enumerate"
  | "display.wake"
  | "remote.desktop.control"
  | "agent.secure_input.recover";

export type SessionCapabilityAction =
  | "sendMessage"
  | "sendReliableClipboardText"
  | "sendFile"
  | "checkAgentPermissions"
  | "enumerateAgentDisplays"
  | "wakeDisplay"
  | "controlRemoteDesktop"
  | "recoverSecureInput";

const ACTION_CAPABILITIES: Record<SessionCapabilityAction, SessionEnhancedCapability[]> = {
  sendMessage: ["message.show"],
  sendReliableClipboardText: ["clipboard.text.write"],
  sendFile: ["file.transfer.upload"],
  checkAgentPermissions: ["agent.permissions.diagnose"],
  enumerateAgentDisplays: ["agent.display.enumerate"],
  wakeDisplay: ["display.wake"],
  controlRemoteDesktop: ["remote.desktop.control"],
  recoverSecureInput: ["agent.secure_input.recover"],
};

export function requiredCapabilitiesForAction(action: SessionCapabilityAction): SessionEnhancedCapability[] {
  return [...ACTION_CAPABILITIES[action]];
}

export function sessionSupportsAction(
  capabilities: CapabilityMatrix | null | undefined,
  action: SessionCapabilityAction,
): boolean {
  return requiredCapabilitiesForAction(action).every((capability) =>
    sessionSupportsCapability(capabilities, capability),
  );
}
