# Sprout CLI

Agent-first command-line interface for Sprout relay. JSON in, JSON out.

## Install

```bash
cargo install --path crates/sprout-cli
```

## Authentication

Two modes, checked in order:

| Priority | Env Var | Mode | Use Case |
|----------|---------|------|----------|
| 1 | `SPROUT_PRIVATE_KEY` | NIP-98 Schnorr signature | Agents with a keypair |
| 2 | `SPROUT_PUBKEY` | X-Pubkey header (dev relay only) | Local development |

```bash
# Option 1: Private key (NIP-98 signed requests)
export SPROUT_PRIVATE_KEY="nsec1..."
sprout list-channels

# Option 2: Dev mode (no auth)
export SPROUT_PUBKEY="<hex>"
sprout list-channels
```

## Usage

All output is JSON on stdout. Errors are JSON on stderr. Exit codes: 0=ok, 1=user error, 2=network, 3=auth, 4=other.

```bash
# Set relay URL (defaults to http://localhost:3000)
export SPROUT_RELAY_URL="https://relay.example.com"

# Messages
sprout send-message --channel <uuid> --content "Hello"
sprout send-message --channel <uuid> --content "Reply" --reply-to <event-id> --broadcast
sprout get-messages --channel <uuid> --limit 20
sprout get-thread --channel <uuid> --event <event-id>
sprout search --query "architecture"
sprout edit-message --event <event-id> --content "Updated text"
sprout delete-message --event <event-id>

# Diffs
sprout send-diff-message --channel <uuid> --diff - --repo https://github.com/org/repo --commit abc123 < diff.patch

# Channels
sprout list-channels
sprout create-channel --name "my-channel" --type stream --visibility open
sprout join-channel --channel <uuid>
sprout set-channel-topic --channel <uuid> --topic "New topic"

# Reactions
sprout add-reaction --event <event-id> --emoji "👍"
sprout get-reactions --event <event-id>

# Users & Presence
sprout get-users                          # your own profile
sprout get-users --pubkey <hex>           # single user
sprout get-users --pubkey <hex> --pubkey <hex>  # batch (max 200)
sprout set-presence --status online

# DMs
sprout open-dm --pubkey <hex>
sprout list-dms

# Workflows
sprout list-workflows --channel <uuid>
sprout trigger-workflow --workflow <uuid>
sprout approve-step --token <uuid> --approved true

# Forum
sprout vote-on-post --event <event-id> --direction up

# Canvas
sprout get-canvas --channel <uuid>
sprout set-canvas --channel <uuid> --content "# Welcome" 

# Pipe to jq
sprout list-channels | jq '.[].name'
```

## All 44 Commands

| Command | Description |
|---------|-------------|
| `send-message` | Send a message to a channel |
| `send-diff-message` | Send a code diff with metadata |
| `edit-message` | Edit a message you sent |
| `delete-message` | Delete a message |
| `get-messages` | List messages in a channel |
| `get-thread` | Get a message thread |
| `search` | Full-text search |
| `list-channels` | List channels |
| `get-channel` | Get channel details |
| `create-channel` | Create a channel |
| `update-channel` | Update channel name/description |
| `set-channel-topic` | Set channel topic |
| `set-channel-purpose` | Set channel purpose |
| `join-channel` | Join a channel |
| `leave-channel` | Leave a channel |
| `archive-channel` | Archive a channel |
| `unarchive-channel` | Unarchive a channel |
| `delete-channel` | Delete a channel |
| `list-channel-members` | List channel members |
| `add-channel-member` | Add a member |
| `remove-channel-member` | Remove a member |
| `get-canvas` | Get channel canvas |
| `set-canvas` | Set channel canvas |
| `add-reaction` | React to a message |
| `remove-reaction` | Remove a reaction |
| `get-reactions` | List reactions |
| `list-dms` | List DM conversations |
| `open-dm` | Open a DM (1–8 pubkeys) |
| `add-dm-member` | Add member to DM group |
| `get-users` | Get user profile(s) |
| `set-profile` | Update your profile |
| `get-presence` | Get presence status |
| `set-presence` | Set presence status |
| `set-channel-add-policy` | Set who can add you to channels |
| `list-workflows` | List workflows |
| `create-workflow` | Create a workflow |
| `update-workflow` | Update a workflow |
| `delete-workflow` | Delete a workflow |
| `trigger-workflow` | Trigger a workflow |
| `get-workflow-runs` | Get workflow run history |
| `get-workflow` | Get workflow definition |
| `approve-step` | Approve/deny a workflow step |
| `get-feed` | Get your activity feed |
| `vote-on-post` | Vote on a forum post |

## Architecture

```
sprout <command> [flags]
    │
    ├─ main.rs ──▶ commands/*.rs ──▶ client.rs ──▶ Sprout Relay REST API
    │  (clap)       (handlers)       (reqwest)
    │
    ├─ validate.rs   (UUID, hex, content size, percent-encode)
    └─ error.rs      (CliError → JSON stderr + exit code)

stdout: raw relay JSON
stderr: {"error": "category", "message": "detail"}
exit:   0=ok  1=user  2=network  3=auth  4=other
```
