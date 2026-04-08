#!/usr/bin/env bash
# Computes the full multi-instance desktop dev environment.
# Source this file from desktop dev commands; it exports:
#   SPROUT_VITE_PORT, SPROUT_HMR_PORT, VITE_PORT, VITE_HMR_PORT
#   SPROUT_RELAY_PORT, SPROUT_RELAY_URL
#   SPROUT_INSTANCE_SLUG, SPROUT_WORKTREE_LABEL, VITE_DEV_BRANCH (worktrees only)
#   SPROUT_TAURI_CONFIG

WORKTREE_ROOT=$(git rev-parse --show-toplevel 2>/dev/null || pwd)

# Derive a stable base port from the worktree root so the same worktree always
# gets the same ports. This keeps the Tauri dev config stable between runs and
# preserves Cargo's build cache.
BASE_PORT=$(python3 -c "import hashlib,sys; h=int(hashlib.sha256(sys.argv[1].encode()).hexdigest(), 16); print(10000 + h % 55000)" "$WORKTREE_ROOT")
export SPROUT_VITE_PORT=$BASE_PORT
export SPROUT_HMR_PORT=$((BASE_PORT + 1))
export SPROUT_RELAY_PORT=3000
export VITE_PORT="$SPROUT_VITE_PORT"
export VITE_HMR_PORT="$SPROUT_HMR_PORT"
export SPROUT_RELAY_URL="${SPROUT_RELAY_URL:-ws://localhost:3000}"

SPROUT_TAURI_CONFIG="{\"build\":{\"devUrl\":\"http://localhost:${SPROUT_VITE_PORT}\",\"beforeDevCommand\":\"exec ./node_modules/.bin/vite --port ${SPROUT_VITE_PORT} --strictPort\"},\"identifier\":\"xyz.block.sprout.app.dev\",\"productName\":\"Sprout Dev\"}"
unset VITE_DEV_BRANCH

# In worktrees, extract a label from the branch name and derive a unique app
# identity and icon so multiple local desktop instances can run side by side.
if git rev-parse --is-inside-work-tree &>/dev/null; then
    GIT_DIR=$(git rev-parse --git-dir)
    if [[ "$GIT_DIR" == *".git/worktrees/"* ]]; then
        BRANCH_NAME=$(git rev-parse --abbrev-ref HEAD)
        export SPROUT_WORKTREE_LABEL="${BRANCH_NAME##*/}"
        export SPROUT_INSTANCE_SLUG=$(echo "$SPROUT_WORKTREE_LABEL" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9]/-/g' | sed 's/--*/-/g' | sed 's/^-//' | sed 's/-$//')

        ICON_DIR="$(pwd)/src-tauri/target/dev-icons"
        mkdir -p "$ICON_DIR"
        DEV_ICON="$ICON_DIR/icon.icns"

        if swift ../scripts/generate-dev-icon.swift src-tauri/icons/icon.icns "$DEV_ICON" "$SPROUT_WORKTREE_LABEL"; then
            echo "🌳 Worktree: ${SPROUT_WORKTREE_LABEL}"
            export VITE_DEV_BRANCH="$SPROUT_WORKTREE_LABEL"
            SPROUT_TAURI_CONFIG="{\"build\":{\"devUrl\":\"http://localhost:${SPROUT_VITE_PORT}\",\"beforeDevCommand\":\"exec ./node_modules/.bin/vite --port ${SPROUT_VITE_PORT} --strictPort\"},\"identifier\":\"xyz.block.sprout.app.dev.${SPROUT_INSTANCE_SLUG}\",\"productName\":\"Sprout Dev (${SPROUT_WORKTREE_LABEL})\",\"bundle\":{\"icon\":[\"$DEV_ICON\"]}}"
        fi
    fi
fi

export SPROUT_TAURI_CONFIG
