# sprout-cli Live Testing Guide

Manual testing runbook for verifying every CLI command against a local relay.
An agent or developer follows this step by step, running each command and
checking the output.

---

## 1. Prerequisites

Docker services running and healthy:

```bash
docker compose ps
# sprout-postgres   healthy
# sprout-redis      healthy
# sprout-typesense  healthy
```

If not running: `./scripts/dev-setup.sh` from the repo root.

Tools: `jq`, `curl`, Rust toolchain.

---

## 2. Build the CLI

```bash
cargo build -p sprout-cli
```

Use `cargo run -p sprout-cli --` or the built binary at `target/debug/sprout`.

---

## 3. Start the Relay

In a separate terminal:

```bash
cd REPOS/sprout-nostr
set -a && source .env && set +a
cargo run -p sprout-relay
```

Verify:

```bash
curl -s http://localhost:3000/_liveness
# "ok" or 200 status
```

The `.env` should have `SPROUT_REQUIRE_AUTH_TOKEN=false` for local dev.

---

## 4. Mint Test Credentials

### Option A: sprout-admin (full scopes including admin)

This mints a token with all CLI-relevant scopes (including `admin:channels`)
via direct DB access. Use this for testing admin operations (archive,
delete-channel, add/remove-channel-member).

```bash
DATABASE_URL=postgres://sprout:sprout_dev@localhost:5432/sprout \
cargo run -p sprout-admin -- mint-token \
  --name "cli-test" \
  --scopes "messages:read,messages:write,channels:read,channels:write,users:read,users:write,files:read,files:write,admin:channels"
```

This generates a keypair and prints:
- **Private key (nsec)** — save for `SPROUT_PRIVATE_KEY` testing
- **API Token** — save as `SPROUT_API_TOKEN`
- **Pubkey** — save for `SPROUT_PUBKEY` testing

Export:

```bash
export SPROUT_RELAY_URL="http://localhost:3000"
export SPROUT_API_TOKEN="sprout_tok_..."
export SPROUT_PRIVATE_KEY="nsec1..."   # from the mint output
```

### Option B: sprout auth (NIP-98, self-mintable scopes only)

Tests the CLI's own auth flow. Cannot mint `admin:channels`.

```bash
export SPROUT_PRIVATE_KEY="nsec1..."
export SPROUT_RELAY_URL="http://localhost:3000"
cargo run -p sprout-cli -- auth
# Prints a token string to stdout
```

### Scope reference

| Scope | Self-mintable | Needed for |
|-------|:---:|------------|
| `messages:read` | ✅ | get-messages, get-thread, search, get-feed |
| `messages:write` | ✅ | send-message, edit-message, delete-message, reactions, vote |
| `channels:read` | ✅ | list-channels, get-channel, list-members |
| `channels:write` | ✅ | create-channel, update-channel, join, leave, topic, purpose |
| `users:read` | ✅ | get-users, get-presence |
| `users:write` | ✅ | set-profile, set-presence, set-channel-add-policy |
| `files:read` | ✅ | — |
| `files:write` | ✅ | — |
| `admin:channels` | ❌ | archive, unarchive, delete-channel, add/remove-channel-member |

**Use Option A for full testing.** Option B covers most commands but skips
admin operations.

---

## 5. Unit Tests

```bash
cargo test -p sprout-cli
# Expected: 38 passed, 0 failed

cargo clippy -p sprout-cli -- -D warnings
# Expected: zero warnings
```

---

## 6. Live Testing — Command by Command

Run each command, verify exit code 0 and check output. Most commands
return JSON (pipe through `jq .` to validate). Exceptions: `auth` prints
a raw token string, and `delete-token`/`delete-all-tokens` may return
empty (204). Commands are ordered so earlier ones create resources that
later ones need.

### 6.1 Auth & Tokens

```bash
# list-tokens — list existing tokens
sprout list-tokens | jq .

# auth — mint a new token (requires SPROUT_PRIVATE_KEY)
SPROUT_PRIVATE_KEY="nsec1..." sprout auth
# Should print: sprout_tok_...

# delete-token — delete a specific token by UUID
# ⚠️  Do NOT delete the token you're currently using (SPROUT_API_TOKEN).
# Mint a throwaway token first, then delete it:
THROWAWAY=$(sprout auth)  # mint a new token
THROWAWAY_LIST=$(SPROUT_API_TOKEN="$THROWAWAY" sprout list-tokens)
# Filter by name to avoid deleting the wrong token
THROWAWAY_ID=$(echo "$THROWAWAY_LIST" | jq -r '[.[] // .tokens[] | select(.name == "sprout-cli")][0].id // empty')
sprout delete-token --id "$THROWAWAY_ID"
# May return 204 (empty) or JSON — both are success

# delete-all-tokens — DESTRUCTIVE, deletes all tokens for this pubkey
# sprout delete-all-tokens
# ⚠️  Only run this if you're about to re-mint
```

### 6.2 Channels

```bash
# create-channel (stream)
sprout create-channel --name "test-stream" --type stream --visibility open \
  --description "CLI test channel" | jq .
# Save the channel ID:
CHANNEL_ID=$(sprout create-channel --name "test-cli" --type stream --visibility open | jq -r '.id')

# create-channel (forum) — needed for vote-on-post later
FORUM_ID=$(sprout create-channel --name "test-forum" --type forum --visibility open | jq -r '.id')

# list-channels
sprout list-channels | jq .
sprout list-channels --visibility open | jq .
sprout list-channels --member | jq .

# get-channel
sprout get-channel --channel "$CHANNEL_ID" | jq .

# update-channel
sprout update-channel --channel "$CHANNEL_ID" --name "test-cli-updated" \
  --description "Updated" | jq .

# set-channel-topic
sprout set-channel-topic --channel "$CHANNEL_ID" --topic "Test topic" | jq .

# set-channel-purpose
sprout set-channel-purpose --channel "$CHANNEL_ID" --purpose "Testing" | jq .

# join-channel (may already be a member from create)
sprout join-channel --channel "$CHANNEL_ID" | jq .

# leave-channel
sprout leave-channel --channel "$CHANNEL_ID" | jq .

# Re-join so we can send messages
sprout join-channel --channel "$CHANNEL_ID" | jq .

# archive-channel (requires admin:channels scope)
sprout archive-channel --channel "$CHANNEL_ID" | jq .

# unarchive-channel
sprout unarchive-channel --channel "$CHANNEL_ID" | jq .
```

### 6.3 Canvas

```bash
# set-canvas
sprout set-canvas --channel "$CHANNEL_ID" --content "# Test Canvas" | jq .

# set-canvas from stdin
echo "# Canvas from stdin" | sprout set-canvas --channel "$CHANNEL_ID" --content - | jq .

# get-canvas
sprout get-canvas --channel "$CHANNEL_ID" | jq .
```

### 6.4 Messages

```bash
# send-message
MSG=$(sprout send-message --channel "$CHANNEL_ID" --content "Hello from CLI test" | jq .)
echo "$MSG"
EVENT_ID=$(echo "$MSG" | jq -r '.id // .event_id')

# send-message with reply + broadcast
REPLY=$(sprout send-message --channel "$CHANNEL_ID" --content "Reply" \
  --reply-to "$EVENT_ID" --broadcast | jq .)
echo "$REPLY"
REPLY_ID=$(echo "$REPLY" | jq -r '.id // .event_id')

# send-message with mentions
sprout send-message --channel "$CHANNEL_ID" --content "Hey @someone" \
  --mention "0000000000000000000000000000000000000000000000000000000000000001" | jq .

# get-messages
sprout get-messages --channel "$CHANNEL_ID" | jq .
sprout get-messages --channel "$CHANNEL_ID" --limit 5 | jq .

# get-thread
sprout get-thread --channel "$CHANNEL_ID" --event "$EVENT_ID" | jq .

# search
sprout search --query "Hello" | jq .
sprout search --query "CLI test" --limit 5 | jq .

# edit-message
sprout edit-message --event "$EVENT_ID" --content "Edited by CLI test" | jq .

# delete-message
sprout delete-message --event "$REPLY_ID" | jq .
```

### 6.5 Diff Messages

```bash
# send-diff-message from stdin
echo '--- a/foo.rs
+++ b/foo.rs
@@ -1,3 +1,3 @@
-fn old() {}
+fn new() {}' | sprout send-diff-message \
  --channel "$CHANNEL_ID" \
  --diff - \
  --repo "https://github.com/example/repo" \
  --commit "abcdef1234567890abcdef1234567890abcdef12" | jq .

# send-diff-message with metadata
echo "diff content" | sprout send-diff-message \
  --channel "$CHANNEL_ID" \
  --diff - \
  --repo "https://github.com/example/repo" \
  --commit "abcdef1234567890abcdef1234567890abcdef12" \
  --file "src/main.rs" \
  --lang "rust" \
  --description "Refactored main" | jq .

# send-diff-message with branch + PR metadata
echo "diff content" | sprout send-diff-message \
  --channel "$CHANNEL_ID" \
  --diff - \
  --repo "https://github.com/example/repo" \
  --commit "abcdef1234567890abcdef1234567890abcdef12" \
  --parent-commit "1234567890abcdef1234567890abcdef12345678" \
  --source-branch "feature/cli" \
  --target-branch "main" \
  --pr 42 | jq .
```

### 6.6 Reactions

```bash
# Send a message to react to
REACT_MSG=$(sprout send-message --channel "$CHANNEL_ID" --content "React to this")
REACT_ID=$(echo "$REACT_MSG" | jq -r '.id // .event_id')

# add-reaction
sprout add-reaction --event "$REACT_ID" --emoji "👍" | jq .

# get-reactions
sprout get-reactions --event "$REACT_ID" | jq .

# remove-reaction
sprout remove-reaction --event "$REACT_ID" --emoji "👍" | jq .
```

### 6.7 DMs

```bash
# list-dms
sprout list-dms | jq .

# open-dm (needs a real pubkey — use your own or a test one)
# Get your own pubkey first:
MY_PUBKEY=$(sprout get-users | jq -r '.pubkey // .[0].pubkey // empty')
echo "My pubkey: $MY_PUBKEY"

# open-dm with a synthetic pubkey (relay will create the user)
DM_RESULT=$(sprout open-dm --pubkey "0000000000000000000000000000000000000000000000000000000000000001")
echo "$DM_RESULT" | jq .
DM_ID=$(echo "$DM_RESULT" | jq -r '.channel_id // .id // empty')

# add-dm-member (requires messages:write scope — NOT admin:channels)
sprout add-dm-member --channel "$DM_ID" \
  --pubkey "0000000000000000000000000000000000000000000000000000000000000002" | jq .
```

### 6.8 Users & Presence

```bash
# get-users — own profile (0 pubkeys)
sprout get-users | jq .

# get-users — single pubkey
sprout get-users --pubkey "$MY_PUBKEY" | jq .

# get-users — batch (2+ pubkeys)
sprout get-users --pubkey "$MY_PUBKEY" --pubkey "$MY_PUBKEY" | jq .

# set-profile
sprout set-profile --name "CLI Test Agent" --about "Testing sprout-cli" | jq .

# get-presence
sprout get-presence --pubkeys "$MY_PUBKEY" | jq .

# set-presence
sprout set-presence --status online | jq .
sprout set-presence --status away | jq .
sprout set-presence --status offline | jq .

# set-channel-add-policy
sprout set-channel-add-policy --policy anyone | jq .
sprout set-channel-add-policy --policy owner_only | jq .
sprout set-channel-add-policy --policy nobody | jq .
# Reset to default
sprout set-channel-add-policy --policy anyone | jq .
```

### 6.9 Channel Members (add/remove require admin:channels)

```bash
# add-channel-member
sprout add-channel-member --channel "$CHANNEL_ID" \
  --pubkey "0000000000000000000000000000000000000000000000000000000000000001" \
  --role member | jq .

# list-channel-members
sprout list-channel-members --channel "$CHANNEL_ID" | jq .

# remove-channel-member
sprout remove-channel-member --channel "$CHANNEL_ID" \
  --pubkey "0000000000000000000000000000000000000000000000000000000000000001" | jq .
```

### 6.10 Workflows

```bash
# create-workflow
# NOTE: trigger uses `on:` tag (serde internally tagged enum).
# Valid triggers: message_posted, reaction_added, diff_posted, schedule, webhook
# Steps use `action:` tag: send_message, send_dm, set_channel_topic, add_reaction, etc.
WF=$(sprout create-workflow --channel "$CHANNEL_ID" \
  --yaml 'name: test-wf
trigger:
  on: webhook
steps:
  - id: step1
    action: send_message
    text: "Hello from workflow"' | jq .)
echo "$WF"
WF_ID=$(echo "$WF" | jq -r '.id')

# list-workflows
sprout list-workflows --channel "$CHANNEL_ID" | jq .

# get-workflow
sprout get-workflow --workflow "$WF_ID" | jq .

# update-workflow
sprout update-workflow --workflow "$WF_ID" \
  --yaml 'name: test-wf-updated
trigger:
  on: webhook
steps:
  - id: step1
    action: send_message
    text: "Updated"' | jq .

# trigger-workflow
sprout trigger-workflow --workflow "$WF_ID" | jq .

# get-workflow-runs
sprout get-workflow-runs --workflow "$WF_ID" | jq .

# approve-step — requires a workflow run waiting for approval
# This is hard to test ad-hoc without a workflow that has an approval gate.
# Test the validation instead:
sprout approve-step --token "00000000-0000-0000-0000-000000000000" --approved true 2>&1 || true
# Should fail with relay error (token not found), not a validation error

# delete-workflow
sprout delete-workflow --workflow "$WF_ID" | jq .
```

### 6.11 Feed

```bash
sprout get-feed | jq .
sprout get-feed --limit 5 | jq .
```

### 6.12 Forum & Voting

```bash
# Send a forum post (kind 45001) to the forum channel
FORUM_POST=$(sprout send-message --channel "$FORUM_ID" \
  --content "Forum post for vote testing" --kind 45001 | jq .)
echo "$FORUM_POST"
FORUM_EVENT_ID=$(echo "$FORUM_POST" | jq -r '.id // .event_id')

# vote-on-post (up)
sprout vote-on-post --event "$FORUM_EVENT_ID" --direction up | jq .

# vote-on-post (down)
sprout vote-on-post --event "$FORUM_EVENT_ID" --direction down | jq .
```

---

## 7. Error Path Testing

Verify the CLI produces correct JSON on stderr and correct exit codes.

```bash
# Exit 1: Invalid UUID
sprout get-channel --channel "not-a-uuid" 2>&1; echo "exit: $?"
# stderr: {"error":"user_error","message":"invalid UUID: not-a-uuid"}
# exit: 1

# Exit 1: Invalid hex64
sprout delete-message --event "not-hex" 2>&1; echo "exit: $?"
# stderr: {"error":"user_error","message":"must be a 64-character hex string: not-hex"}
# exit: 1

# Exit 1: Invalid --approved value
sprout approve-step --token "00000000-0000-0000-0000-000000000000" \
  --approved maybe 2>&1; echo "exit: $?"
# stderr: {"error":"user_error","message":"--approved must be 'true' or 'false' (got: maybe)"}
# exit: 1

# Exit 1: Invalid --type value
sprout create-channel --name x --type invalid --visibility open 2>&1; echo "exit: $?"
# stderr: {"error":"user_error","message":"--type must be 'stream' or 'forum' (got: invalid)"}
# exit: 1

# Exit 1: Invalid --direction value
sprout vote-on-post --event "$(printf '0%.0s' {1..64})" \
  --direction sideways 2>&1; echo "exit: $?"
# exit: 1

# Exit 1: Empty body guard
sprout set-profile 2>&1; echo "exit: $?"
# exit: 1 (at least one field required)

# Exit 3: No auth configured
env -u SPROUT_API_TOKEN -u SPROUT_PRIVATE_KEY -u SPROUT_PUBKEY \
  cargo run -p sprout-cli -- list-channels 2>&1; echo "exit: $?"
# stderr: {"error":"auth_error","message":"auth error: Set SPROUT_API_TOKEN, SPROUT_PRIVATE_KEY, or SPROUT_PUBKEY"}
# exit: 3

# Exit 2: Non-existent channel (valid UUID)
sprout get-channel --channel "00000000-0000-0000-0000-000000000000" 2>&1; echo "exit: $?"
# stderr: {"error":"relay_error","message":"..."}
# exit: 2
```

---

## 8. Auth Mode Testing

Test all three authentication tiers.

```bash
# Mode 1: Bearer token (SPROUT_API_TOKEN)
SPROUT_API_TOKEN="sprout_tok_..." sprout list-channels | jq .
# Should succeed

# Mode 2: Private key auto-mint (SPROUT_PRIVATE_KEY)
SPROUT_PRIVATE_KEY="nsec1..." sprout list-channels | jq .
# Should succeed (mints a 1-day token at startup)

# Mode 3: Dev mode (SPROUT_PUBKEY) — only works with SPROUT_REQUIRE_AUTH_TOKEN=false
SPROUT_PUBKEY="<your-64-char-hex-pubkey>" sprout list-channels | jq .
# Should succeed

# No auth → exit 3
env -u SPROUT_API_TOKEN -u SPROUT_PRIVATE_KEY -u SPROUT_PUBKEY \
  cargo run -p sprout-cli -- list-channels 2>&1; echo "exit: $?"
# exit: 3
```

---

## 9. Cleanup

```bash
# Delete test channels
sprout delete-channel --channel "$CHANNEL_ID" | jq .
sprout delete-channel --channel "$FORUM_ID" | jq .
```

---

## 10. Checklist

| # | Command | Tested | Notes |
|---|---------|:------:|-------|
| 1 | `send-message` | ☐ | Basic, reply, broadcast, mentions |
| 2 | `send-diff-message` | ☐ | Stdin, metadata, branch/PR |
| 3 | `edit-message` | ☐ | |
| 4 | `delete-message` | ☐ | |
| 5 | `get-messages` | ☐ | With limit |
| 6 | `get-thread` | ☐ | |
| 7 | `search` | ☐ | With limit |
| 8 | `list-channels` | ☐ | With visibility, member |
| 9 | `get-channel` | ☐ | |
| 10 | `create-channel` | ☐ | Stream and forum |
| 11 | `update-channel` | ☐ | |
| 12 | `set-channel-topic` | ☐ | |
| 13 | `set-channel-purpose` | ☐ | |
| 14 | `join-channel` | ☐ | |
| 15 | `leave-channel` | ☐ | |
| 16 | `archive-channel` | ☐ | Needs admin:channels |
| 17 | `unarchive-channel` | ☐ | Needs admin:channels |
| 18 | `delete-channel` | ☐ | Needs admin:channels |
| 19 | `list-channel-members` | ☐ | |
| 20 | `add-channel-member` | ☐ | Needs admin:channels |
| 21 | `remove-channel-member` | ☐ | Needs admin:channels |
| 22 | `get-canvas` | ☐ | |
| 23 | `set-canvas` | ☐ | Direct and stdin |
| 24 | `add-reaction` | ☐ | |
| 25 | `remove-reaction` | ☐ | |
| 26 | `get-reactions` | ☐ | |
| 27 | `list-dms` | ☐ | |
| 28 | `open-dm` | ☐ | |
| 29 | `add-dm-member` | ☐ | Needs messages:write |
| 30 | `get-users` | ☐ | Self, single, batch |
| 31 | `set-profile` | ☐ | |
| 32 | `get-presence` | ☐ | |
| 33 | `set-presence` | ☐ | online, away, offline |
| 34 | `set-channel-add-policy` | ☐ | anyone, owner_only, nobody |
| 35 | `list-workflows` | ☐ | |
| 36 | `create-workflow` | ☐ | |
| 37 | `update-workflow` | ☐ | |
| 38 | `delete-workflow` | ☐ | |
| 39 | `trigger-workflow` | ☐ | |
| 40 | `get-workflow-runs` | ☐ | |
| 41 | `get-workflow` | ☐ | |
| 42 | `approve-step` | ☐ | Validation only (needs approval gate) |
| 43 | `get-feed` | ☐ | |
| 44 | `vote-on-post` | ☐ | Up and down |
| 45 | `auth` | ☐ | Mint token via NIP-98 |
| 46 | `list-tokens` | ☐ | |
| 47 | `delete-token` | ☐ | |
| 48 | `delete-all-tokens` | ☐ | Optional (destructive) |
