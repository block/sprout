# Releasing Sprout Desktop

## Quick Start

```sh
# Regular release (next patch version)
just release

# Patch release
just release patch

# Minor release
just release 0.4.0

# Any explicit version
just release 1.0.0
```

This creates a `version-bump/<version>` PR that bumps all version manifests, regenerates lockfiles, and appends a changelog entry. Merge the PR to trigger the build automatically.

---

## How It Works

1. **`just release`** runs locally on `main` — computes the next version, creates a `version-bump/<version>` branch, bumps versions in all manifests, regenerates lockfiles, generates a changelog entry, commits, pushes, and opens a PR.

2. **Merge the PR** — the `auto-tag-on-release-pr-merge` workflow detects the `version-bump/*` branch merge and pushes a `v<version>` tag.

3. **Tag triggers `release.yml`** — the existing release workflow builds, signs, notarizes, and publishes the desktop app for macOS and Linux.

---

## Release Types

| Command | Version | Example |
|---------|---------|---------|
| `just release` | Next patch | `0.3.0` → `0.3.1` |
| `just release patch` | Next patch | `0.3.0` → `0.3.1` |
| `just release 0.4.0` | Explicit minor | `0.3.1` → `0.4.0` |
| `just release 1.0.0` | Explicit | `1.0.0` |

---

## Version Files

`just bump-version <version>` updates these files:

| File | Field |
|------|-------|
| `desktop/package.json` | `"version"` |
| `desktop/src-tauri/tauri.conf.json` | `"version"` |
| `desktop/src-tauri/Cargo.toml` | `version` (under `[package]`) |
| `mobile/pubspec.yaml` | `version:` (preserves build number) |

It also regenerates `pnpm-lock.yaml`, `desktop/src-tauri/Cargo.lock`, and `mobile/pubspec.lock`.

---

## Manual Fallback

If the automated flow isn't suitable (e.g., building from a non-main ref):

1. Go to **Actions > Release** in the GitHub UI
2. Click **Run workflow**
3. Provide the semver version (no `v` prefix) and the ref to build from

---

## Internal Releases

After the OSS release ships, trigger an internal build via the
[sprout-releases Buildkite pipeline](https://buildkite.com/runway/sprout-releases).
See the [sprout-releases README](https://github.com/squareup/sprout-releases#cutting-a-release)
for the full step-by-step instructions and input field reference.

---

## What Gets Published

Each release produces two GitHub releases:

1. **`v<version>`** — the user-facing release with the `.dmg` installer
   (macOS).

2. **`sprout-desktop-latest`** — a rolling pre-release for the Tauri
   auto-updater containing `latest.json`, the signed `.tar.gz` archive,
   and its `.sig` signature.

---

## Prerequisites

- **Write access** to the `block/sprout` GitHub repository
- **`gh` CLI** authenticated (`gh auth status`)
- The following **GitHub Actions secrets** must be configured:

  | Secret | Purpose |
  |--------|---------|
  | `SPROUT_UPDATER_PUBLIC_KEY` | Tauri updater public key (minisign) |
  | `TAURI_SIGNING_PRIVATE_KEY` | Tauri updater private key |
  | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for the private key |

---

## Troubleshooting

### `just release` fails with "must be on main branch"
Switch to `main` and pull latest before running `just release`.

### `just release` fails with "working tree is dirty"
Commit or stash your changes before running `just release`.

### Build fails at "Validate version"
The version string must be valid semver: `MAJOR.MINOR.PATCH` with an optional pre-release suffix. Do not include a `v` prefix.

### Auto-updater reports "no update available"
Verify that the `sprout-desktop-latest` release exists and contains a valid `latest.json`.
