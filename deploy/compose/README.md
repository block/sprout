# Buzz Docker Compose deployment

This is the single-node/VPS deployment bundle. It is intentionally separate from
the root `docker-compose.yml`, which remains local development infrastructure.

## Quick start

```bash
cd deploy/compose
cp .env.example .env
$EDITOR .env       # replace every CHANGE_ME value
./run.sh start
```

For a public VPS with automatic Let's Encrypt certificates:

```bash
cd deploy/compose
BUZZ_COMPOSE_TLS=true ./run.sh start
```

Normal users should eventually run Dawn's bootstrap script instead of editing
`.env` by hand. The bootstrap is responsible for generating stable secrets and,
optionally, an owner keypair.

## Production notes

- Default `BUZZ_IMAGE` tracks `ghcr.io/block/buzz:main` for early testing. Pin it to `ghcr.io/block/buzz:sha-<7>` or a semver release tag for production once available.
- Keep `BUZZ_RELAY_PRIVATE_KEY`, `BUZZ_GIT_HOOK_HMAC_SECRET`, database/Redis,
  Typesense, and S3 secrets stable across restarts.
- `RELAY_OWNER_PUBKEY` is intentionally not prefixed with `BUZZ_`; it must be a
  64-character hex Nostr pubkey when closed relay mode is enabled.
- `BUZZ_AUTO_MIGRATE=true` requires an image that includes embedded SQLx
  migrations. Do not use this bundle for a fresh public install until that
  image has been published.
- The stack uses Postgres, Redis, Typesense, MinIO, and a git data volume because
  those are real Buzz dependencies today. Minimal mode can simplify this later.

Run `./run.sh backup-hint` for the backup checklist.

## Validation

Before sharing an install link publicly, verify a fresh install with:

```bash
cd deploy/compose
cp .env.example .env
$EDITOR .env
./run.sh config
./run.sh start
curl -fsS http://127.0.0.1:${BUZZ_HTTP_PORT:-3000}/_liveness
./run.sh status
```
