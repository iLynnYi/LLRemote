type SessionSurfaceIdentity = {
  sessionId: string;
  owner: "embedded" | "external";
  windowLabel: string;
};

function normalizeSurfacePart(value: string): string {
  return value.trim().replace(/:/g, "-");
}

export function createSessionSurfaceId(identity: SessionSurfaceIdentity): string {
  return [
    "surface",
    normalizeSurfacePart(identity.sessionId),
    identity.owner,
    normalizeSurfacePart(identity.windowLabel),
  ].join(":");
}
