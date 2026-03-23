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
| `APPLE_CERTIFICATE`               | Base64-encoded `.p12` Apple Developer certificate                  |
| `APPLE_CERTIFICATE_PASSWORD`      | Password for the `.p12` certificate                                |
| `APPLE_SIGNING_IDENTITY`          | e.g. `"Developer ID Application: Block, Inc. (XXXXXXXXXX)"`       |
| `APPLE_ID`                        | Apple ID email for notarization                                    |
| `APPLE_PASSWORD`                  | App-specific password for notarization                             |
| `APPLE_TEAM_ID`                   | Apple Developer Team ID                                            |

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
4. **Builds and signs** the app via `tauri-action`.
5. **Notarizes** the macOS bundle with Apple.
6. **Publishes** the updater manifest (`latest.json`) to the rolling
   `sprout-desktop-latest` release.
7. **Publishes** the DMG to both the versioned and rolling releases.

---

## Local Release Build (Testing)

To test a release build locally:

```bash
# Set required env vars
export SPROUT_UPDATER_PUBLIC_KEY="your-public-key"
export SPROUT_UPDATER_ENDPOINT="https://github.com/block/sprout/releases/download/sprout-desktop-latest/latest.json"
export APPLE_SIGNING_IDENTITY="Developer ID Application: ..."

# Generate release config
cd desktop
pnpm run tauri:release:config

# Build
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

- **Notarization failures**: Verify `APPLE_ID`, `APPLE_PASSWORD` (must be an
  app-specific password), and `APPLE_TEAM_ID` are correct.

- **Signing failures**: Verify the `.p12` certificate is valid and not expired,
  and that `APPLE_CERTIFICATE_PASSWORD` is correct.

- **Version mismatch**: The tag version must exactly match all three version
  files (`package.json`, `tauri.conf.json`, `Cargo.toml`). Use
  `just desktop-prepare` to ensure consistency.
