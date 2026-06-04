import manifestJson from "../../../../features.json";
import type { FeatureDefinition, FeaturesManifest } from "./types";

const manifest = manifestJson as FeaturesManifest;

/** All features defined in the manifest */
export const allFeatures: FeatureDefinition[] = manifest.features;

/** Only features available on desktop */
export const desktopFeatures: FeatureDefinition[] = manifest.features.filter(
  (f) => !f.platforms || f.platforms.includes("desktop"),
);

/** Look up a feature by id */
export function getFeature(id: string): FeatureDefinition | undefined {
  return manifest.features.find((f) => f.id === id);
}
