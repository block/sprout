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

Agents commit under their own identity. The human operator signs off for accountability.

- **Author/Committer:** use the agent's own name and email. Configure repo-local or use `GIT_AUTHOR_NAME`/`GIT_AUTHOR_EMAIL`/`GIT_COMMITTER_NAME`/`GIT_COMMITTER_EMAIL`. Format: `Agent-Name <agent-id@users.noreply.github.com>` (or relay-based email if no GitHub account exists for the agent).
- **Human sign-off (required):** every commit MUST include a `Signed-off-by` trailer for the human operator who is responsible for the agent's work. Add via `git commit --trailer "Signed-off-by: Human Name <human@email>"`. One blank line must separate trailers from the commit body.
- **Signing:** if the agent has a registered signing key, sign commits. If not, commits will land unverified — this is acceptable until agent SSH keys are provisioned. Do NOT use the human's signing key.
- **Verify before pushing:** `git log -1` should show the agent as author and the human's `Signed-off-by` trailer.

<!-- BEGIN SPROUT MANAGED — regenerated automatically, do not edit below -->
## Active Agents

*(No agents deployed yet. Add agents in the Sprout desktop app.)*

<!-- END SPROUT MANAGED -->
