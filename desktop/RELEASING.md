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

### 1. Prepare

From `main`, run:

```bash
just desktop-prepare <version>
```

For example:

```bash
just desktop-prepare 0.2.0
```

This creates a release branch, bumps versions in `package.json`,
`tauri.conf.json`, and `Cargo.toml`, and opens a PR.

### 2. Review & Merge

Review the PR, ensure CI passes, then merge to `main`.

### 3. Release

From `main` (after pulling the merged changes), run:

```bash
just desktop-release <version>
```

This tags the commit and pushes the tag — CI handles the rest.

### 4. Verify

Check GitHub Releases for:

- The **versioned release** (e.g. `sprout-desktop-v0.2.0`)
- The **`sprout-desktop-latest` rolling release** (updated with every release)

---

## What CI Does

The `sprout-desktop-release.yml` workflow:

1. **Validates** the tag version matches the version in `package.json`,
   `tauri.conf.json`, and `Cargo.toml`.
2. **Validates** all required secrets are present.
3. **Builds** the release config with signing and updater settings.
4. **Builds** the Tauri app (unsigned).
5. **Signs and notarizes** the macOS bundle via `block/apple-codesign-action`.
6. **Re-packages** the signed app into a DMG and updater archive.
7. **Signs** the updater archive with the Tauri updater key.
8. **Publishes** the updater manifest (`latest.json`) to the rolling
   `sprout-desktop-latest` release.
9. **Publishes** the DMG to both the versioned and rolling releases.

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

# Build (unsigned)
just desktop-release-build
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

- **Release builds**: Set this to the production relay URL (e.g.
  `wss://relay.sprout.example.com`). Configure it in the environment before
  building, or set it in the CI workflow.
- **Development**: If not set, it defaults to `ws://localhost:3000`.

---

## Troubleshooting

- **"Missing required desktop release secrets"**: Ensure all secrets listed in
  [Prerequisites](#prerequisites--secrets-setup) are configured in GitHub repo
  settings.

- **Codesigning failures**: Verify `OSX_CODESIGN_ROLE` and
  `CODESIGN_S3_BUCKET` are configured correctly. Check the
  `block/apple-codesign-action` step logs for details.

- **Version mismatch**: The tag version must exactly match all three version
  files (`package.json`, `tauri.conf.json`, `Cargo.toml`). Use
  `just desktop-prepare` to ensure consistency.
