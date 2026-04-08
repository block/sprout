#!/usr/bin/env bash
# Computes per-worktree instance identity: ports, labels, slugs.
# Source this file; it exports:
#   SPROUT_VITE_PORT, SPROUT_HMR_PORT  (per-worktree, hash-derived)
#   SPROUT_RELAY_PORT                   (fixed — all instances share one relay)
#   SPROUT_INSTANCE_SLUG, SPROUT_WORKTREE_LABEL (only if in a git worktree)

# Derive a stable base port from the working directory so the same worktree
# always gets the same ports. This avoids changing TAURI_CONFIG between
# runs, which would invalidate Cargo's build cache and trigger a full
# Rust rebuild every time.
BASE_PORT=$(python3 -c "import hashlib,os; h=int(hashlib.sha256(os.getcwd().encode()).hexdigest(),16); print(10000 + h % 55000)")
export SPROUT_VITE_PORT=$BASE_PORT
export SPROUT_HMR_PORT=$((BASE_PORT + 1))
export SPROUT_RELAY_PORT=3000

# In worktrees, extract a label from the branch name
if git rev-parse --is-inside-work-tree &>/dev/null; then
    GIT_DIR=$(git rev-parse --git-dir)
    if [[ "$GIT_DIR" == *".git/worktrees/"* ]]; then
        BRANCH_NAME=$(git rev-parse --abbrev-ref HEAD)
        export SPROUT_WORKTREE_LABEL="${BRANCH_NAME##*/}"
        # Sanitize for use in bundle identifiers (lowercase, alphanumeric + hyphens)
        export SPROUT_INSTANCE_SLUG=$(echo "$SPROUT_WORKTREE_LABEL" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9]/-/g' | sed 's/--*/-/g' | sed 's/^-//' | sed 's/-$//')
    fi
fi
