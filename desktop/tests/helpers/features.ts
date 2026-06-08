// Single source of truth for E2E tests: derive the preview-feature list from
// /features.json so we don't have to hand-maintain a parallel array.
//
// Tier transitions (preview → stable, or new preview features added) are
// picked up automatically by every test that imports from here.
import featuresManifest from "../../../features.json" with { type: "json" };

interface FeatureDefinition {
  id: string;
  name: string;
  description: string;
  tier: "stable" | "preview";
  platforms?: string[];
}

interface FeaturesManifest {
  version: number;
  features: FeatureDefinition[];
}

const manifest = featuresManifest as FeaturesManifest;

/** IDs of every preview-tier feature on desktop. */
export const PREVIEW_FEATURE_IDS: string[] = manifest.features
  .filter((f) => f.tier === "preview")
  .filter((f) => !f.platforms || f.platforms.includes("desktop"))
  .map((f) => f.id);
