import type { Translate } from "@app/types";
import type { EventLogLevel } from "@diagnostics/eventLog";
import type { SessionConnectionModel } from "@session/connection-model/connectionModel";
import type { LocalVncServerSessionPreparation } from "../useSessionLocalServerControls.ts";

export function logSessionTransportResolution(options: {
  target: string;
  localPreparation: LocalVncServerSessionPreparation | undefined;
  connectionModel: SessionConnectionModel;
  logEvent: (level: EventLogLevel, source: string, message: string) => void;
  t: Translate;
}) {
  options.logEvent("info", options.t("log.sourceSession"), [
    "Session connection model prepared:",
    `target=${options.target}`,
    `base_transport=${options.connectionModel.baseTransport.kind}`,
    `server_identity=${options.connectionModel.serverIdentity.kind}`,
    `identity_verification=${options.connectionModel.serverIdentity.verification}`,
    `enhanced_channel=${options.connectionModel.enhancedChannel.status}`,
    `local_endpoint_match=${options.localPreparation?.localEndpointMatch.state ?? "not-matched"}`,
  ].join(" "));
}
