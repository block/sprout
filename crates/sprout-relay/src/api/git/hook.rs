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
set -e

# Collect ref updates from stdin
REFS=""
while read old_oid new_oid ref_name; do
    # Determine if this is a fast-forward (ancestry check)
    ZERO="0000000000000000000000000000000000000000"
    IS_ANCESTOR="false"
    if [ "$old_oid" != "$ZERO" ] && [ "$new_oid" != "$ZERO" ]; then
        # CRITICAL: git merge-base inherits GIT_OBJECT_DIRECTORY and
        # GIT_ALTERNATE_OBJECT_DIRECTORIES automatically (they're in our env).
        # Exit 0 = is ancestor (FF), exit 1 = not ancestor (NFF), exit 128 = error (treat as NFF).
        if git merge-base --is-ancestor "$old_oid" "$new_oid" 2>/dev/null; then
            IS_ANCESTOR="true"
        fi
    fi

    # Build JSON entry (no jq dependency — manual construction)
    if [ -n "$REFS" ]; then
        REFS="${REFS},"
    fi
    REFS="${REFS}{\"old_oid\":\"${old_oid}\",\"new_oid\":\"${new_oid}\",\"ref_name\":\"${ref_name}\",\"is_ancestor\":${IS_ANCESTOR}}"
done

# Compute timestamp
TIMESTAMP=$(date +%s)

# Build HMAC payload: repo_id|pusher_pubkey|sorted_refs_concat|timestamp
# Sort refs by ref_name for deterministic HMAC
SORTED_REFS=$(echo "$REFS" | tr ',' '\n' | sort -t'"' -k8 | tr '\n' ',')
SORTED_REFS="${SORTED_REFS%,}"

# Compute HMAC-SHA256 signature
# Payload format matches relay's compute_hmac: repo_id|pusher|old+new+ref per sorted ref|timestamp
HMAC_INPUT="${SPROUT_REPO_ID}|${SPROUT_PUSHER_PUBKEY}|"
# Extract oids and refs in sorted order for HMAC
for entry in $(echo "$SORTED_REFS" | tr ',' ' '); do
    OLD=$(echo "$entry" | sed 's/.*"old_oid":"\([^"]*\)".*/\1/')
    NEW=$(echo "$entry" | sed 's/.*"new_oid":"\([^"]*\)".*/\1/')
    REF=$(echo "$entry" | sed 's/.*"ref_name":"\([^"]*\)".*/\1/')
    HMAC_INPUT="${HMAC_INPUT}${OLD}${NEW}${REF}"
done
HMAC_INPUT="${HMAC_INPUT}|${TIMESTAMP}"

SIGNATURE=$(printf '%s' "$HMAC_INPUT" | openssl dgst -sha256 -hmac "$SPROUT_HOOK_SECRET" -hex 2>/dev/null | sed 's/.*= //')

# Build request body
BODY="{\"repo_id\":\"${SPROUT_REPO_ID}\",\"pusher_pubkey\":\"${SPROUT_PUSHER_PUBKEY}\",\"ref_updates\":[${REFS}],\"timestamp\":${TIMESTAMP},\"signature\":\"${SIGNATURE}\"}"

# POST to policy endpoint — FAIL-CLOSED
# --fail: exit non-zero on HTTP errors (4xx, 5xx)
# --max-time 10: timeout after 10s (push is synchronous, relay is local)
# --silent: no progress output
HTTP_CODE=$(curl --fail --silent --max-time 10 \
    -o /tmp/sprout_hook_response.$$ \
    -w "%{http_code}" \
    -X POST \
    -H "Content-Type: application/json" \
    -d "$BODY" \
    "$SPROUT_HOOK_URL" 2>/dev/null) || {
    echo "error: push authorization failed (could not reach policy service)" >&2
    rm -f /tmp/sprout_hook_response.$$
    exit 1
}

if [ "$HTTP_CODE" != "200" ]; then
    echo "error: push denied by policy" >&2
    # Print denial reasons if available
    cat /tmp/sprout_hook_response.$$ >&2 2>/dev/null
    rm -f /tmp/sprout_hook_response.$$
    exit 1
fi

rm -f /tmp/sprout_hook_response.$$
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
