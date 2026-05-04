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
  | `OSX_CODESIGN_ROLE` | AWS IAM role ARN for OIDC authentication with the signing service |
  | `CODESIGN_S3_BUCKET` | S3 bucket used for artifact transfer during signing |

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
- Build the Tauri desktop app
- Sign and notarize the app via Block's Apple code signing service
- Create a versioned GitHub release (`v0.4.0`) with the signed `.dmg` installer
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

## Code Signing (macOS)

OSS release builds are **signed and notarized** via Block's Apple code
signing service (`block/apple-codesign-action`). The app is signed with a
Developer ID certificate and notarized by Apple, so users will not see
Gatekeeper warnings on first launch.

The following additional secrets are required for code signing:

| Secret | Purpose |
|--------|---------|
| `OSX_CODESIGN_ROLE` | AWS IAM role ARN for OIDC authentication with the signing service |
| `CODESIGN_S3_BUCKET` | S3 bucket used for artifact transfer during signing |

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

### Build fails at "Sign with Apple codesigning service"
Check that `OSX_CODESIGN_ROLE` and `CODESIGN_S3_BUCKET` secrets are
configured correctly. The signing service requires valid AWS OIDC
credentials. Also verify the repository has `id-token: write` permission
in the workflow.

### Auto-updater reports "no update available"
Verify that the `sprout-desktop-latest` release exists and contains a
valid `latest.json`. If the user is on Intel Mac, no update will be
found (ARM64 only).
