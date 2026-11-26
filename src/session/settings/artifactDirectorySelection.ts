import type { SessionSettings } from "./sessionSettings.ts";

type DirectoryChoice = { path: string; isBuiltin?: boolean };
type ArtifactDirectorySelection = Pick<SessionSettings, "artifactDirectory" | "artifactDirectoryMode">;

/** The built-in choice follows its current backend path; unavailable custom grants stay selected. */
export function resolveArtifactDirectorySelection(
  current: ArtifactDirectorySelection,
  directories: readonly DirectoryChoice[],
): ArtifactDirectorySelection {
  const builtin = directories.find(directory => directory.isBuiltin);
  const selected = current.artifactDirectoryMode === "builtin" && builtin
    ? builtin : directories.find(directory => directory.path === current.artifactDirectory);
  const directory = selected ?? builtin ?? directories[0];
  return directory ? {
    artifactDirectory: directory.path,
    artifactDirectoryMode: directory.isBuiltin ? "builtin" : "custom",
  } : current;
}
