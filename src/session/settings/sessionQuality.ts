import type { SessionQualityProfile } from "../viewport/sessionDisplay.ts";

export type RfbQualitySettings = {
  qualityLevel: number;
  compressionLevel: number;
};

export function qualityProfileSettings(profile: SessionQualityProfile): RfbQualitySettings {
  if (profile === "sharp") {
    return { qualityLevel: 9, compressionLevel: 0 };
  }

  if (profile === "fast") {
    return { qualityLevel: 4, compressionLevel: 1 };
  }

  return { qualityLevel: 6, compressionLevel: 2 };
}
