import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const publicKey = process.env.SPROUT_UPDATER_PUBLIC_KEY;
const endpoint = process.env.SPROUT_UPDATER_ENDPOINT;
const signingIdentity =
  process.env.SPROUT_APPLE_SIGNING_IDENTITY ??
  process.env.APPLE_SIGNING_IDENTITY;

const baseConfigPath = resolve(process.cwd(), "src-tauri/tauri.conf.json");
const outputConfigPath = resolve(
  process.cwd(),
  "src-tauri/tauri.release.conf.json",
);
const baseConfig = JSON.parse(readFileSync(baseConfigPath, "utf-8"));

const releaseConfig = { ...baseConfig };

releaseConfig.bundle = {
  ...(releaseConfig.bundle ?? baseConfig.bundle ?? {}),
  createUpdaterArtifacts: "v1Compatible",
};

releaseConfig.bundle.macOS = {
  ...(releaseConfig.bundle?.macOS ?? baseConfig.bundle?.macOS ?? {}),
  minimumSystemVersion: "10.15",
};

if (signingIdentity) {
  releaseConfig.bundle.macOS.signingIdentity = signingIdentity;
  console.log(`macOS signing identity configured (${signingIdentity})`);
} else {
  console.log(
    "macOS signing identity skipped (missing: SPROUT_APPLE_SIGNING_IDENTITY or APPLE_SIGNING_IDENTITY)",
  );
}

if (publicKey && endpoint) {
  releaseConfig.plugins = {
    ...(baseConfig.plugins ?? {}),
    updater: {
      pubkey: publicKey,
      endpoints: [endpoint],
    },
  };
  console.log(`Updater config enabled (${endpoint})`);
} else {
  const missing = [];
  if (!publicKey) missing.push("SPROUT_UPDATER_PUBLIC_KEY");
  if (!endpoint) missing.push("SPROUT_UPDATER_ENDPOINT");
  console.log(`Updater config skipped (missing: ${missing.join(", ")})`);
}

writeFileSync(outputConfigPath, `${JSON.stringify(releaseConfig, null, 2)}\n`);
console.log(`Wrote ${outputConfigPath}`);
