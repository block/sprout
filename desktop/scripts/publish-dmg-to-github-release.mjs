import { execFileSync } from "node:child_process";
import { join, resolve } from "node:path";
import { cpSync, existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";

const repo = process.env.GITHUB_REPOSITORY ?? "block/sprout";
const version = requireVersionEnv();
const versionTag = `desktop/v${version}`;
const latestTag = "sprout-desktop-latest";
const dryRun = process.env.DRY_RUN === "true" || process.env.DRY_RUN === "1";

const dmgName = `Sprout_${version}_aarch64.dmg`;
const latestAssetName = "Sprout-latest-aarch64.dmg";

const defaultDmgDirs = [
  "src-tauri/target/aarch64-apple-darwin/release/bundle/dmg",
  "src-tauri/target/release/bundle/dmg",
];
const dmgDir = resolve(
  process.cwd(),
  process.env.DMG_BUNDLE_DIR ??
    defaultDmgDirs.find((dir) =>
      existsSync(resolve(process.cwd(), dir, dmgName)),
    ) ??
    defaultDmgDirs[0],
);
const dmgPath = join(dmgDir, dmgName);

function requireVersionEnv() {
  const v = process.env.VERSION;
  if (!v || !v.trim()) {
    throw new Error(
      "VERSION env var is required. CI sets this from the git tag; for local use, run: VERSION=x.y.z pnpm run release:dmg:publish",
    );
  }
  return v.trim();
}

function quote(arg) {
  if (/^[a-zA-Z0-9._:/=-]+$/.test(arg)) return arg;
  return `'${arg.replace(/'/g, `'\\''`)}'`;
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

function ensureRelease(tag, { title, prerelease }) {
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
  if (prerelease) {
    args.push("--prerelease");
  }

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

function assetUrl(tag, name) {
  return `https://github.com/${repo}/releases/download/${tag}/${encodeURIComponent(name)}`;
}

function main() {
  if (!existsSync(dmgPath)) {
    throw new Error(`Missing DMG artifact: ${dmgPath}`);
  }

  const stageDir = mkdtempSync(join(tmpdir(), "sprout-release-dmg-"));
  const latestAliasPath = join(stageDir, latestAssetName);

  try {
    cpSync(dmgPath, latestAliasPath);

    const uploadVersionedArgs = [
      "release",
      "upload",
      versionTag,
      dmgPath,
      "--repo",
      repo,
      "--clobber",
    ];
    const uploadLatestArgs = [
      "release",
      "upload",
      latestTag,
      latestAliasPath,
      "--repo",
      repo,
      "--clobber",
    ];

    console.log(`Preparing DMG upload for ${repo}`);
    console.log(`- version tag:     ${versionTag}`);
    console.log(`- latest tag:      ${latestTag}`);
    console.log(`- versioned asset: ${dmgName}`);
    console.log(`- latest alias:    ${latestAssetName}`);

    if (dryRun) {
      console.log("DRY_RUN enabled. Skipping upload.");
      console.log(`gh release view ${quote(versionTag)} --repo ${quote(repo)}`);
      console.log(
        `gh release view ${quote(latestTag)} --repo ${quote(repo)} || gh release create ${quote(latestTag)} --repo ${quote(repo)} --title ${quote("Sprout Desktop Latest")} --notes ${quote("Automated release placeholder.")}`,
      );
      console.log(`gh ${uploadVersionedArgs.map(quote).join(" ")}`);
      console.log(`gh ${uploadLatestArgs.map(quote).join(" ")}`);
      console.log(`Versioned URL: ${assetUrl(versionTag, dmgName)}`);
      console.log(`Channel URL:   ${assetUrl(latestTag, latestAssetName)}`);
      return;
    }

    ensureRelease(versionTag, {
      title: `Sprout v${version}`,
      prerelease: false,
    });
    ensureRelease(latestTag, {
      title: "Sprout Desktop Latest",
      prerelease: false,
    });

    runGh(uploadVersionedArgs, { stdio: "inherit" });
    runGh(uploadLatestArgs, { stdio: "inherit" });

    const versionAssets = readReleaseAssets(versionTag);
    const latestAssets = readReleaseAssets(latestTag);
    if (!versionAssets.includes(dmgName)) {
      throw new Error(
        `Release ${versionTag} is missing versioned asset ${dmgName} after upload`,
      );
    }
    if (!latestAssets.includes(latestAssetName)) {
      throw new Error(
        `Release ${latestTag} is missing latest alias asset ${latestAssetName} after upload`,
      );
    }

    console.log("GitHub DMG assets verified.");
    console.log(`Versioned URL: ${assetUrl(versionTag, dmgName)}`);
    console.log(`Channel URL:   ${assetUrl(latestTag, latestAssetName)}`);
  } finally {
    rmSync(stageDir, { recursive: true, force: true });
  }
}

main();
