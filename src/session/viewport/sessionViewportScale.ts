export type SessionViewportScaleMode =
  | "fit"
  | "actual"
  | "scale125"
  | "scale150"
  | "scale200";

export function sessionViewportScaleUsesAutoFit(mode: SessionViewportScaleMode): boolean {
  return mode === "fit";
}

export function sessionViewportScaleFactor(mode: SessionViewportScaleMode): number {
  switch (mode) {
    case "scale125":
      return 1.25;
    case "scale150":
      return 1.5;
    case "scale200":
      return 2;
    case "fit":
    case "actual":
    default:
      return 1;
  }
}
