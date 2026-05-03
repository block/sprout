import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

// Write a tauri.release.conf.json alongside sprout's base tauri.conf.json.
//
// For OSS release builds this script:
// 1. Sets bundle.macOS.minimumSystemVersion = "10.15" for broad compatibility.
// 2. Configures plugins.updater with the public key and endpoint from env vars.
//    Both SPROUT_UPDATER_PUBLIC_KEY and SPROUT_UPDATER_ENDPOINT are required —
//    the script fails if either is missing (OSS builds always ship with updater).
// 3. Sets bundle.createUpdaterArtifacts = true so Tauri automatically produces
//    the .tar.gz archive and .sig signature during the build.

const baseConfigPath = resolve(process.cwd(), "src-tauri/tauri.conf.json");
const outputConfigPath = resolve(
  process.cwd(),
  "src-tauri/tauri.release.conf.json",
);
const baseConfig = JSON.parse(readFileSync(baseConfigPath, "utf-8"));

const releaseConfig = { ...baseConfig };

const updaterPubkey = process.env.SPROUT_UPDATER_PUBLIC_KEY;
const updaterEndpoint = process.env.SPROUT_UPDATER_ENDPOINT;

const missing = [];
if (!updaterPubkey) missing.push("SPROUT_UPDATER_PUBLIC_KEY");
if (!updaterEndpoint) missing.push("SPROUT_UPDATER_ENDPOINT");
if (missing.length > 0) {
  console.error(
    `Error: required environment variable(s) missing: ${missing.join(", ")}`,
  );
  process.exit(1);
}

releaseConfig.bundle = {
  ...(releaseConfig.bundle ?? {}),
  macOS: {
    ...(releaseConfig.bundle?.macOS ?? {}),
    minimumSystemVersion: "10.15",
  },
  createUpdaterArtifacts: true,
};

releaseConfig.plugins = {
  ...(releaseConfig.plugins ?? {}),
  updater: {
    pubkey: updaterPubkey,
    endpoints: [updaterEndpoint],
  },
};

console.log(`Updater enabled -> ${updaterEndpoint}`);

writeFileSync(outputConfigPath, `${JSON.stringify(releaseConfig, null, 2)}\n`);
console.log(`Wrote ${outputConfigPath}`);
