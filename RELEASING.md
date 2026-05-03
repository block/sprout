# Releasing Sprout Desktop

This document describes how to create a new OSS release of the Sprout
desktop app.

---

## Prerequisites

- **Write access** to the `block/sprout` GitHub repository. Only
  collaborators with push permissions can trigger the release workflow.
- The following **GitHub Actions secrets** must be configured on the repo
  (Settings > Secrets and variables > Actions):

  | Secret | Purpose |
  |--------|---------|
  | `SPROUT_UPDATER_PUBLIC_KEY` | Tauri updater public key (minisign) |
  | `TAURI_SIGNING_PRIVATE_KEY` | Tauri updater private key (used to sign the update archive) |
  | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for the private key |

---

## Creating a Release

1. Go to **Actions > Release** in the GitHub UI:\
   `https://github.com/block/sprout/actions/workflows/release.yml`

2. Click **"Run workflow"**.

3. Fill in the inputs:
   - **version** — a semver version string, e.g. `0.4.0` or `1.0.0-beta.1`.
     Do not include a `v` prefix.
   - **ref** — the branch, tag, or commit SHA to build from. Defaults to
     `main`.

4. Click **"Run workflow"** to start the build.

The workflow will:

- Validate the version string
- Check out the specified ref
- Patch the version into `package.json`, `tauri.conf.json`, and `Cargo.toml`
- Build all sidecar binaries (`sprout-acp`, `sprout-mcp`,
  `git-credential-nostr`)
- Build the Tauri desktop app with updater signing enabled
- Create a versioned GitHub release (`v0.4.0`) with the `.dmg` installer
- Update the rolling `sprout-desktop-latest` release with the signed
  update archive and `latest.json` manifest for the auto-updater

---

## What Gets Published

Each release produces two GitHub releases:

1. **`v<version>`** (e.g. `v0.4.0`) — the user-facing release with the
   `.dmg` installer. This is what users download manually.

2. **`sprout-desktop-latest`** — a rolling pre-release used by the Tauri
   auto-updater. Contains `latest.json`, the signed `.tar.gz` archive,
   and its `.sig` signature. Users should not download from this release
   directly.

---

## Platform Support

The release workflow currently builds for **macOS ARM64 only**
(`darwin-aarch64`). Intel Mac (`darwin-x86_64`) support would require
adding a matrix build to the workflow.

---

## Auto-Updater

The desktop app checks for updates by fetching `latest.json` from the
`sprout-desktop-latest` release:

```
https://github.com/block/sprout/releases/download/sprout-desktop-latest/latest.json
```

When a new version is available, the app downloads the signed archive,
verifies the signature against the embedded public key, and applies the
update.

---

## Troubleshooting

### Build fails at "Validate version"
The version string must be valid semver: `MAJOR.MINOR.PATCH` with an
optional pre-release suffix (e.g. `1.0.0-beta.1`). Do not include a `v`
prefix.

### Build fails at "Build Tauri app"
Check that the signing secrets are configured correctly. The build
requires `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to be set.

### Auto-updater reports "no update available"
Verify that the `sprout-desktop-latest` release exists and contains a
valid `latest.json`. If the user is on Intel Mac, no update will be
found (ARM64 only).
