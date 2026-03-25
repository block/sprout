# Releasing the Sprout Desktop App

This guide covers the end-to-end process for releasing the Sprout desktop app,
including secrets setup, cutting releases, and troubleshooting.

---

## Prerequisites / Secrets Setup

The following GitHub repository secrets must be configured before the first
release:

| Secret                             | Description                                                        |
| ---------------------------------- | ------------------------------------------------------------------ |
| `SPROUT_UPDATER_PUBLIC_KEY`        | Tauri updater public key (generate with `pnpm tauri signer generate`) |
| `TAURI_SIGNING_PRIVATE_KEY`        | Tauri updater private key                                          |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for the private key                                     |
| `OSX_CODESIGN_ROLE`               | IAM role ARN for Block's Apple codesigning service                 |
| `CODESIGN_S3_BUCKET`              | S3 bucket for codesigning artifacts                                |

---

## Generating Tauri Updater Keys

```bash
cd desktop
pnpm tauri signer generate -w ~/.tauri/sprout.key
```

This generates a keypair:

- The **public key** goes in the `SPROUT_UPDATER_PUBLIC_KEY` secret.
- The **private key** goes in the `TAURI_SIGNING_PRIVATE_KEY` secret.

Store the password you chose in `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

---

## Cutting a Release

From any branch (typically `main`):

```bash
just desktop-release 0.3.0
```

Or equivalently:

```bash
git tag desktop/v0.3.0
git push origin desktop/v0.3.0
```

That's it. CI extracts the version from the tag and writes it into
`package.json`, `tauri.conf.json`, and `Cargo.toml` at build time. The
versions checked into the repo are not used for releases — the tag is the
source of truth.

### Verify

Check GitHub Releases for:

- The **versioned release** (e.g. `desktop/v0.3.0`)
- The **`sprout-desktop-latest` rolling release** (updated with every release)

---

## What CI Does

The `sprout-desktop-release.yml` workflow:

1. **Extracts the version** from the git tag once into `RELEASE_VERSION`.
2. **Sets the version** into `package.json`, `tauri.conf.json`, and
   `Cargo.toml` using `set-version-from-tag.mjs`.
3. **Regenerates `Cargo.lock`** to match the patched `Cargo.toml`.
4. **Validates** all required secrets are present.
5. **Builds** the release config with signing and updater settings.
6. **Builds** the Tauri app (unsigned).
7. **Signs and notarizes** the macOS bundle via `block/apple-codesign-action`.
8. **Re-packages** the signed app into a DMG and updater archive.
9. **Signs** the updater archive with the Tauri updater key.
10. **Publishes** the updater manifest (`latest.json`) to the rolling
    `sprout-desktop-latest` release.
11. **Publishes** the DMG to both the versioned and rolling releases.

---

## Local Release Build (Testing)

Local builds will not be codesigned or notarized — that only happens in CI
via `block/apple-codesign-action`. Local builds are useful for testing the
updater config and DMG packaging.

```bash
# Set updater env vars
export SPROUT_UPDATER_PUBLIC_KEY="your-public-key"
export SPROUT_UPDATER_ENDPOINT="https://github.com/block/sprout/releases/download/sprout-desktop-latest/latest.json"

# Generate release config
cd desktop
pnpm run tauri:release:config

# Build (unsigned) — pass a version to set it before building
just desktop-release-build version=0.3.0
```

You can also set the version separately without building:

```bash
just desktop-set-version 0.3.0
```

---

## Auto-Updates

The app uses `tauri-plugin-updater` to check for updates. The updater endpoint
is:

```
https://github.com/block/sprout/releases/download/sprout-desktop-latest/latest.json
```

This `latest.json` is updated on every release and contains the download URL
and signature for the latest version.

---

## Relay URL Configuration

The app connects to the relay via the `SPROUT_RELAY_URL` environment variable.

- **Production releases**: The GitHub release workflow currently builds the app
  with `SPROUT_RELAY_URL=wss://sprout-oss.stage.blox.sqprod.co`, which is baked
  into the release binary as its default relay URL.
- **Local release builds**: Export `SPROUT_RELAY_URL` before running
  `just desktop-release-build` if you want a non-localhost relay URL compiled
  into the app.
- **Development**: If not set, it defaults to `ws://localhost:3000`.

---

## How Versioning Works

The git tag is the single source of truth for the release version. The version
fields in `package.json`, `tauri.conf.json`, and `Cargo.toml` on `main` are
**not** used for releases — CI overwrites them at build time from the tag.

This means the tagged commit will show a different version in its source files
than what the release actually contains. This is an accepted tradeoff — the tag
is the canonical version, the commit is just the code state at release time.
This is standard practice in ecosystems like Docker, Go, and Rust where the
tag drives the version.

---

## Troubleshooting

- **"Missing required desktop release secrets"**: Ensure all secrets listed in
  [Prerequisites](#prerequisites--secrets-setup) are configured in GitHub repo
  settings.

- **Codesigning failures**: Verify `OSX_CODESIGN_ROLE` and
  `CODESIGN_S3_BUCKET` are configured correctly. Check the
  `block/apple-codesign-action` step logs for details.

- **Build failures**: If versions are wrong, check that the tag follows the
  format `desktop/v<semver>` (e.g. `desktop/v0.3.0`). CI extracts the version
  from the tag automatically.
