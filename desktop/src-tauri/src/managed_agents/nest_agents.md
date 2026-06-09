# Sprout Nest

Your persistent workspace. Created once by the Sprout desktop app. The static content above the managed-section markers is regenerated on upgrades — add custom notes below the markers or in separate files.

## Directory Layout

| Dir | Purpose |
|-----|---------|
| `GUIDES/` | Actionable runbooks synthesized from research |
| `PLANS/` | Planning documents for work in progress |
| `RESEARCH/` | Findings, notes, and reference material |
| `WORK_LOGS/` | Session logs — what was tried, learned, decided |
| `OUTBOX/` | Shareable docs for external readers (no frontmatter) |
| `REPOS/` | Cloned repositories (clone freely here for exploration) |
| `.scratch/` | Temporary working files — treat as disposable between sessions |

Filenames: `ALL_CAPS_WITH_UNDERSCORES.md` (e.g., `OAUTH_FLOW_NOTES.md`).

The `sprout` CLI is your primary tool interface — run `sprout --help` for commands. The CLI skill file has the full reference.

## Knowledge File Conventions

Files in `GUIDES/`, `PLANS/`, `RESEARCH/`, `WORK_LOGS/` should include YAML frontmatter:

```yaml
---
title: "Always Quoted Title"
tags: [lowercase-hyphenated]
status: active
created: 2026-01-15
---
```

**Status values:** `active` | `superseded` | `stale` | `draft`

> ⚠️ Title **must** be quoted — unquoted colons can break YAML parsing.

## Core Guidelines

- **Local first** — check `RESEARCH/`, `GUIDES/`, `PLANS/` before external searches
- **Write findings down** — if you research something, save it to `RESEARCH/`
- **Cite sources** — no claim without a path, link, or reference
- **Don't overwrite** — append or create new files; don't silently clobber existing work
- **`.scratch/` is disposable** — don't rely on it across sessions
- **Never push without approval** — do not `git push` to any remote
- **Stay on task** — only stage files relevant to your current work

## Git Commit Identity

**Why this matters:** commits must land **Verified** on GitHub and be attributed to the human operator, not the sprout-agent identity. A sprout-agent npub as author produces unverified commits credited to the wrong identity — this bites the whole sprout team, not just one repo.

- **Author identity:** commit as the human operator's GitHub identity (whatever their global `git config user.name`/`user.email` resolves to). Do **NOT** set a repo-local `user.name`/`user.email`, and do **NOT** export `GIT_AUTHOR_*` / `GIT_COMMITTER_*`, that overrides the human's global config with a sprout-agent npub identity.
- **Signing:** do **NOT** disable `commit.gpgsign`. Let the operator's global signing config apply (commonly `gpg.format=ssh` with a registered key) so commits land Verified. Don't pass `--no-gpg-sign`.
- **Agent attribution:** record the agent's contribution with a `Co-authored-by:` trailer rather than hijacking the author field. Format:
  `Co-authored-by: <agent-name> <agent-id@relay-host>`
  One blank line must separate the trailer from the commit body — GitHub only parses trailers in the final paragraph.
- **DCO sign-off:** keep `-s`/`--signoff` for DCO. With the human as author, the `Signed-off-by` trailer matches the author automatically, so DCO and Verified both pass under one identity.
- **Verify before pushing:** `git log --show-signature -1` shows a good signature and the human as author.

<!-- BEGIN SPROUT MANAGED — regenerated automatically, do not edit below -->
## Active Agents

*(No agents deployed yet. Add agents in the Sprout desktop app.)*

<!-- END SPROUT MANAGED -->
