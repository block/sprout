import { execFileSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";

const repo = process.env.GITHUB_REPOSITORY ?? "block/sprout";
const tauriConfigPath = resolve(process.cwd(), "src-tauri/tauri.conf.json");
const version = process.env.VERSION ?? readVersionFromConfig();
const latestTag = "sprout-desktop-latest";
const tauriTarget = process.env.TAURI_TARGET ?? "aarch64-apple-darwin";
const updaterPlatform = process.env.UPDATER_PLATFORM ?? "darwin-aarch64";
const dryRun = process.env.DRY_RUN === "true" || process.env.DRY_RUN === "1";

const defaultBundleDirs = [
  `src-tauri/target/${tauriTarget}/release/bundle/macos`,
  "src-tauri/target/release/bundle/macos",
];
const bundleDir = resolve(
  process.cwd(),
  process.env.UPDATER_BUNDLE_DIR ??
    defaultBundleDirs.find((dir) => existsSync(resolve(process.cwd(), dir))) ??
    defaultBundleDirs[0],
);
const latestPath = join(bundleDir, "latest.json");

function readVersionFromConfig() {
  const config = JSON.parse(readFileSync(tauriConfigPath, "utf-8"));
  const configVersion = config?.version;
  if (typeof configVersion !== "string" || !configVersion.trim()) {
    throw new Error(`Could not determine version from ${tauriConfigPath}`);
  }
  return configVersion;
}

function requirePath(path) {
  if (!existsSync(path)) {
    throw new Error(`Missing required file: ${path}`);
  }
}

function runGh(args, options = {}) {
  return execFileSync("gh", args, options);
}

function releaseExists(tag) {
  try {
    runGh(["release", "view", tag, "--repo", repo], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

function ensureRelease(tag, title) {
  if (releaseExists(tag)) {
    return;
  }

  const args = [
    "release",
    "create",
    tag,
    "--repo",
    repo,
    "--title",
    title,
    "--notes",
    "Automated release placeholder.",
  ];

  runGh(args, { stdio: "inherit" });
}

function readReleaseAssets(tag) {
  const assetsRaw = runGh(
    [
      "release",
      "view",
      tag,
      "--repo",
      repo,
      "--json",
      "assets",
      "--jq",
      ".assets[].name",
    ],
    { encoding: "utf-8" },
  );
  return assetsRaw
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

function downloadUrl(name) {
  return `https://github.com/${repo}/releases/download/${latestTag}/${encodeURIComponent(name)}`;
}

function resolveArchivePath() {
  const canonicalArchiveName = "Sprout.app.tar.gz";
  const canonicalPath = join(bundleDir, canonicalArchiveName);
  if (existsSync(canonicalPath)) {
    return canonicalPath;
  }

  const candidates = readdirSync(bundleDir).filter((entry) =>
    entry.endsWith(".app.tar.gz"),
  );
  if (candidates.length === 1) {
    return join(bundleDir, candidates[0]);
  }
  if (candidates.length === 0) {
    throw new Error(
      `Could not find updater archive in ${bundleDir}. Expected ${canonicalArchiveName}.`,
    );
  }
  throw new Error(
    `Found multiple updater archives in ${bundleDir}: ${candidates.join(", ")}. Cannot determine which to use.`,
  );
}

function buildLatestJson(signaturePath) {
  const signature = readFileSync(signaturePath, "utf-8").trim();
  return {
    version,
    notes: `Release v${version}.`,
    pub_date: new Date().toISOString(),
    platforms: {
      [updaterPlatform]: {
        signature,
        url: "",
      },
    },
  };
}

function main() {
  const archivePath = resolveArchivePath();
  const signaturePath = `${archivePath}.sig`;
  requirePath(archivePath);
  requirePath(signaturePath);

  const latest = existsSync(latestPath)
    ? JSON.parse(readFileSync(latestPath, "utf-8"))
    : buildLatestJson(signaturePath);
  latest.version = version;
  latest.pub_date = new Date().toISOString();

  const platformRecord = latest?.platforms?.[updaterPlatform];
  if (!platformRecord) {
    const available = Object.keys(latest?.platforms ?? {});
    throw new Error(
      `Platform "${updaterPlatform}" missing in latest.json. Available: ${available.join(", ") || "(none)"}`,
    );
  }

  const archiveName = basename(archivePath);
  const signatureName = basename(signaturePath);
  platformRecord.signature = readFileSync(signaturePath, "utf-8").trim();
  platformRecord.url = downloadUrl(archiveName);

  const stageDir = mkdtempSync(join(tmpdir(), "sprout-github-updater-"));
  const stagedLatestPath = join(stageDir, "latest.json");

  try {
    cpSync(archivePath, join(stageDir, archiveName));
    cpSync(signaturePath, join(stageDir, signatureName));
    writeFileSync(stagedLatestPath, `${JSON.stringify(latest, null, 2)}\n`);

    const uploadArgs = [
      "release",
      "upload",
      latestTag,
      stagedLatestPath,
      join(stageDir, archiveName),
      join(stageDir, signatureName),
      "--repo",
      repo,
      "--clobber",
    ];

    console.log(`Preparing updater upload for ${repo}`);
    console.log(`- latest tag:       ${latestTag}`);
    console.log(`- updater archive:  ${archiveName}`);
    console.log(`- updater endpoint: ${downloadUrl("latest.json")}`);

    if (dryRun) {
      console.log("DRY_RUN enabled. Skipping upload.");
      console.log(
        `gh release view ${latestTag} --repo ${repo} || gh release create ${latestTag} --repo ${repo} --title "Sprout Desktop Latest" --notes "Automated release placeholder."`,
      );
      console.log(`gh ${uploadArgs.join(" ")}`);
      return;
    }

    ensureRelease(latestTag, "Sprout Desktop Latest");
    runGh(uploadArgs, { stdio: "inherit" });

    const latestAssets = readReleaseAssets(latestTag);
    for (const expected of ["latest.json", archiveName, signatureName]) {
      if (!latestAssets.includes(expected)) {
        throw new Error(
          `Release ${latestTag} is missing ${expected} after upload`,
        );
      }
    }

    console.log("GitHub updater assets verified.");
    console.log(`Updater endpoint: ${downloadUrl("latest.json")}`);
  } finally {
    rmSync(stageDir, { recursive: true, force: true });
  }
}

main();
