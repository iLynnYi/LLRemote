import { getName, getVersion } from "@tauri-apps/api/app";
import { isTauri } from "@tauri-apps/api/core";
import tauriConfig from "../../src-tauri/tauri.conf.json";

export type AppMetadata = {
  name: string;
  version: string;
  description: string;
};

const bundledMetadata: AppMetadata = {
  name: tauriConfig.productName,
  version: tauriConfig.version,
  description: tauriConfig.bundle.shortDescription ?? "",
};

export function fallbackAppMetadata(): AppMetadata {
  return bundledMetadata;
}

/** Reads the installed app's name and version; browser preview uses the bundled config. */
export async function loadAppMetadata(): Promise<AppMetadata> {
  if (!isTauri()) {
    return bundledMetadata;
  }

  try {
    const [name, version] = await Promise.all([getName(), getVersion()]);
    return { ...bundledMetadata, name, version };
  } catch {
    return bundledMetadata;
  }
}
