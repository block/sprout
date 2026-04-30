//! Pre-receive hook script generation and injection.
//!
//! The hook is a shell script that:
//! 1. Reads `old_oid new_oid ref_name` lines from stdin
//! 2. For each non-create/non-delete, runs `git merge-base --is-ancestor`
//!    (inheriting quarantine env vars)
//! 3. POSTs the payload to the relay's internal policy endpoint with HMAC
//! 4. Exits non-zero on ANY non-200 response (fail-closed)
//!
//! Security invariants:
//! - Fail-closed: curl failure, timeout, non-200 → exit 1
//! - Quarantine vars inherited for ancestry checks
//! - HMAC binds callback to specific push operation

use std::path::Path;

use tokio::fs;
use tracing::{error, info};

/// The pre-receive hook script content.
///
/// Environment variables set by the relay before spawning git receive-pack:
/// - `SPROUT_HOOK_URL` — internal policy endpoint (http://127.0.0.1:{port}/internal/git/policy)
/// - `SPROUT_HOOK_SECRET` — per-push HMAC secret
/// - `SPROUT_REPO_ID` — repo identifier (d-tag)
/// - `SPROUT_PUSHER_PUBKEY` — authenticated pusher's hex pubkey
///
/// Git sets automatically (quarantine):
/// - `GIT_OBJECT_DIRECTORY` — quarantine object store
/// - `GIT_ALTERNATE_OBJECT_DIRECTORIES` — includes the real object store
const PRE_RECEIVE_HOOK: &str = r#"#!/bin/sh
# Sprout pre-receive hook — FAIL-CLOSED
# ANY error, timeout, or non-200 response → reject the push.
set -eo pipefail

ZERO="0000000000000000000000000000000000000000"
TMPDIR="${TMPDIR:-/tmp}"
REFS_FILE="$TMPDIR/sprout_hook_refs.$$"
HMAC_FILE="$TMPDIR/sprout_hook_hmac.$$"
RESP_FILE="$TMPDIR/sprout_hook_resp.$$"
trap 'rm -f "$REFS_FILE" "$HMAC_FILE" "$HMAC_FILE.concat" "$RESP_FILE"' EXIT

# Phase 1: Read ref updates from stdin, classify each, build JSON + HMAC lines.
# We write two files in parallel:
#   REFS_FILE: JSON entries (unsorted, for the request body)
#   HMAC_FILE: "ref_name old_oid new_oid" lines (for sorting → HMAC input)
REFS=""
while read old_oid new_oid ref_name; do
    # Ancestry check for FF detection.
    # CRITICAL: GIT_OBJECT_DIRECTORY and GIT_ALTERNATE_OBJECT_DIRECTORIES are
    # inherited from our environment (git sets them for quarantine). Any git
    # subprocess we call sees the quarantined objects automatically.
    IS_ANCESTOR="false"
    if [ "$old_oid" != "$ZERO" ] && [ "$new_oid" != "$ZERO" ]; then
        # Exit 0 = is ancestor (FF), exit 1 = not ancestor (NFF),
        # exit 128 = error → treat as NFF (fail-closed).
        if git merge-base --is-ancestor "$old_oid" "$new_oid" 2>/dev/null; then
            IS_ANCESTOR="true"
        fi
    fi

    # JSON entry for request body.
    if [ -n "$REFS" ]; then
        REFS="${REFS},"
    fi
    REFS="${REFS}{\"old_oid\":\"${old_oid}\",\"new_oid\":\"${new_oid}\",\"ref_name\":\"${ref_name}\",\"is_ancestor\":${IS_ANCESTOR}}"

    # HMAC line: ref_name first (for sorting), then oids.
    echo "${ref_name} ${old_oid} ${new_oid}" >> "$HMAC_FILE"
done

# Phase 2: Compute HMAC-SHA256 signature.
# Payload format MUST match relay's compute_hmac() in policy.rs:
#   repo_id | pusher_pubkey | (old_oid + new_oid + ref_name) per ref sorted by ref_name | timestamp
TIMESTAMP=$(date +%s)

HMAC_INPUT="${SPROUT_REPO_ID}|${SPROUT_PUSHER_PUBKEY}|"
# Sort by ref_name (field 1) — matches Rust's sort_by(|a, b| a.ref_name.cmp(&b.ref_name))
if [ -f "$HMAC_FILE" ]; then
    sort "$HMAC_FILE" | while IFS=' ' read ref_name old_oid new_oid; do
        printf '%s%s%s' "$old_oid" "$new_oid" "$ref_name"
    done > "$HMAC_FILE.concat"
    HMAC_INPUT="${HMAC_INPUT}$(cat "$HMAC_FILE.concat")"
    rm -f "$HMAC_FILE.concat"
fi
HMAC_INPUT="${HMAC_INPUT}|${TIMESTAMP}"

SIGNATURE=$(printf '%s' "$HMAC_INPUT" | openssl dgst -sha256 -hmac "$SPROUT_HOOK_SECRET" -hex 2>/dev/null | sed 's/.*= //')
if [ -z "$SIGNATURE" ]; then
    echo "error: failed to compute HMAC signature" >&2
    exit 1
fi

# Phase 3: POST to policy endpoint — FAIL-CLOSED.
BODY="{\"repo_id\":\"${SPROUT_REPO_ID}\",\"pusher_pubkey\":\"${SPROUT_PUSHER_PUBKEY}\",\"ref_updates\":[${REFS}],\"timestamp\":${TIMESTAMP},\"signature\":\"${SIGNATURE}\"}"

HTTP_CODE=$(curl --fail --silent --max-time 10 \
    -o "$RESP_FILE" \
    -w "%{http_code}" \
    -X POST \
    -H "Content-Type: application/json" \
    -d "$BODY" \
    "$SPROUT_HOOK_URL" 2>/dev/null) || {
    echo "error: push authorization failed (could not reach policy service)" >&2
    exit 1
}

if [ "$HTTP_CODE" != "200" ]; then
    echo "error: push denied by policy" >&2
    cat "$RESP_FILE" >&2 2>/dev/null
    exit 1
fi

exit 0
"#;

/// Install the pre-receive hook into a bare repository.
///
/// Creates a `hooks/` directory and writes the hook script with execute permission.
/// Called during repo creation (kind:30617 handling) and can be called to
/// retrofit existing repos.
pub async fn install_hook(repo_path: &Path) -> anyhow::Result<()> {
    let hooks_dir = repo_path.join("hooks");
    fs::create_dir_all(&hooks_dir).await.map_err(|e| {
        error!(path = %hooks_dir.display(), error = %e, "failed to create hooks dir");
        anyhow::anyhow!("failed to create hooks directory: {e}")
    })?;

    let hook_path = hooks_dir.join("pre-receive");
    fs::write(&hook_path, PRE_RECEIVE_HOOK).await.map_err(|e| {
        error!(path = %hook_path.display(), error = %e, "failed to write hook");
        anyhow::anyhow!("failed to write pre-receive hook: {e}")
    })?;

    // Make executable (Unix only).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&hook_path, perms).map_err(|e| {
            error!(path = %hook_path.display(), error = %e, "failed to chmod hook");
            anyhow::anyhow!("failed to set hook permissions: {e}")
        })?;
    }

    info!(repo = %repo_path.display(), "pre-receive hook installed");
    Ok(())
}
