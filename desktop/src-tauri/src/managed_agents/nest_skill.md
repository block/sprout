---
name: sprout-cli
description: >
  Use the Sprout CLI (`sprout` command) to interact with a Sprout relay:
  messages, channels, canvas, reactions, DMs, users, workflows, feed, social
  notes, repos, file uploads, and persistent agent memory. Activate for any
  task involving a Sprout relay via the `sprout` command.
version: 1
---

# Sprout CLI

`sprout` talks to a Sprout relay. The CLI is self-documenting — **lean on
`--help`** for command details and only rely on this skill for the conventions
and gotchas that `--help` doesn't surface.

## Discovering commands

```bash
sprout --help                 # 13 command groups + global flags + exit codes
sprout messages --help        # subcommands of a group
sprout messages send --help   # flags + worked examples for one subcommand
```

Every leaf command's help lists its required flags and shows real examples.
When unsure of a flag, check `--help` rather than guessing. The 13 groups:
`messages channels canvas reactions dms users workflows feed social repos
upload mem pack`.

## Environment

`SPROUT_PRIVATE_KEY` and `SPROUT_AUTH_TAG` are pre-set by the harness; auth is
automatic (NIP-98). Never prompt for, read, or echo the key. `SPROUT_RELAY_URL`
defaults to `http://localhost:3000` — override only if told to. `pack` is local
and needs no relay.

## Parameter conventions

- `--channel` / `--workflow` / `--token`: UUID (`550e8400-...`).
- `--event` / `--pubkey` / `--mention`: **64-char lowercase hex**. Convert
  `note1...` / `npub...` Bech32 first — they are rejected.
- `--content -`, `--diff -`, `--yaml -`: read from stdin (pipe-friendly).
- Content max 65,536 bytes; diffs max 61,440 (auto-truncated at a hunk boundary).
- IDs flow forward: `channels create` → `channel_id`, `dms open` → `dm_id`
  (use as `--channel`), `workflows create` → `workflow_id`.

## Output contract

Default is JSON on stdout — arrays for lists, objects for single resources,
`null` when not found. Most **writes** return `{event_id, accepted, message}`.
`--format compact` trims read results to essential fields (global flag).

Asymmetries worth knowing (the rest is plain JSON):
- `canvas get` → raw markdown string, not JSON.
- `social *` and `repos *` → **raw Nostr event JSON** (includes `sig`, relay
  fields) — not the normalized shape used elsewhere.
- `mem get`/`mem hash` → raw value/hex to stdout, no trailing newline.
- `mem set/patch/rm` → progress on **stderr**, nothing on stdout.
- `upload file` → pretty-printed `BlobDescriptor` `{url, sha256, size, ...}`.
- `pack inspect` → human-readable text.

## Errors & exit codes

Errors are `{"error": "<category>", "message": "<detail>"}` on **stderr**.
Exit: `0` ok · `1` bad input · `2` relay/network · `3` auth · `4` other ·
`5` write conflict (superseded). On non-zero exit, read stderr before retrying.
For `mem`, a `5` means someone else wrote first — re-fetch and retry.

## Reading & polling

The relay has **no push/webhooks** — poll. List commands return oldest-first,
**except `feed get`** (mentions of you), which is newest-first. Pattern:

1. `sprout messages get --channel <UUID> --limit 50` — note max `created_at`.
2. Sleep 10–30s (never under 5s — rate limits).
3. `sprout messages get --channel <UUID> --since <max_created_at> --limit 50`.
4. Repeat, advancing `--since` each pass.

`messages get --before <ts>` pages backward; `messages thread --event <id>`
fetches a reply tree; `messages search --query` searches across channels.

## Common gotchas

- Reply/thread with `--reply-to <event-id>` (not `--parent`).
- `messages send --kind`: omit/`9` = stream, `45001` = forum post,
  `45003` = forum comment (needs `--reply-to`); others rejected.
- `users get` always returns an **array**, even for one profile.
- `users set-presence` currently fails (needs WebSocket; CLI uses HTTP).
- `mem patch` is safer than `mem set` under concurrency: `mem hash <slug>`
  first, pass `--base-hash <hex>`. The `core` slug can't be deleted.
- Multi-line content with `$`, backticks, or `*`: pipe a quoted heredoc
  (`<<'EOF'`) into `--content -` so the shell doesn't expand it.
