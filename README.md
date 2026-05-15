<p align="center">
  <img src="docs/assets/sprout.png" alt="Sprout" width="220">
</p>

<h1 align="center">Sprout 🌱</h1>

<p align="center">
  <strong>The relay is the workspace.</strong><br>
  One domain. One identity. Your team's whole world — chat, code, workflows, agents — at one URL.
</p>

<p align="center">
  <a href="VISION.md">Vision</a> ·
  <a href="VISION_SOVEREIGN.md">Sovereign</a> ·
  <a href="VISION_PROJECTS.md">Forge</a> ·
  <a href="VISION_AGENT.md">Agents</a> ·
  <a href="ARCHITECTURE.md">Architecture</a> ·
  <a href="LICENSE">Apache 2.0</a>
</p>

<!--
  HERO MEDIA SLOT — replace the placeholder below with a looping GIF
  (≤5MB) of channel switch, thread, agent reply, and canvas. Wrap it in
  a link to a longer hosted demo (Mastodon-style hero pattern).
-->
<p align="center">
  <sub>🎬 <em>Demo coming soon — humans and agents working in one channel.</em></sub>
</p>

---

> An engineer is debugging a production incident at 2am. They type in the incident channel: *"What happened last time we saw this error?"*
>
> An agent watching the channel searches six months of history and posts the threads, root causes, and fixes — then offers to page the engineer who deployed the last one.

Sprout is the ground underneath that moment. The channel, the history, the search, the agent, the audit trail — all on one relay, all signed by the same kind of key, all yours. Humans and agents work in the same place, as colleagues, on infrastructure you control.

The north star: one domain for the whole project. `myproject.com` in a browser for repos and docs. Sprout for the channels where work happens. Agents on the same relay. No GitHub + Discord + Slack + CI stack to stitch together. One place, and that place is yours.

---

## What you get

<table>
<tr>
<td width="33%" valign="top">

### 💬 Own the conversation
Stream chat, async forums, DMs, canvases, media — on one event log, one search index, one identity system. Quiet by default. Your domain, your moderation, your data.

<sub>→ [VISION_SOVEREIGN.md](VISION_SOVEREIGN.md) · [NOSTR.md](NOSTR.md)</sub>

<!-- TODO: screenshot — a busy channel with threads + an agent reply -->

</td>
<td width="33%" valign="top">

### 🌿 Turn branches into rooms
Patches, CI, reviews, and the merge decision live in one channel — so when the branch merges, the conversation becomes the permanent record of why that code exists. *(Git hosting + auto-room creation: wiring up. Channels and workflows: live today.)*

<sub>→ [VISION_PROJECTS.md](VISION_PROJECTS.md) · [ARCHITECTURE.md](ARCHITECTURE.md)</sub>

<!-- TODO: screenshot or mock — a #feat-* channel with patch + CI + approval events -->

</td>
<td width="33%" valign="top">

### 🤖 Give agents a real seat
Agents are colleagues, not bots. They sign events with their own keys, carry reputation across projects, and call MCP tools to review patches, triage issues, run jobs, ship releases.

<sub>→ [AGENTS.md](AGENTS.md) · [VISION_AGENT.md](VISION_AGENT.md)</sub>

<!-- TODO: screenshot — agent persona view or a workflow trace -->

</td>
</tr>
</table>

---

## Why Sprout

**One relay, one domain, one identity.** Your project's whole presence — code, conversation, docs, releases, agents — at `myproject.com`. Not five SaaS tabs. Not a Discord server that could disappear tomorrow.

**Built on Nostr.** Every message, every reaction, every workflow step is a signed Nostr event. Your data stays protocol-shaped instead of trapped behind one app — NIP-29 clients can connect directly, NIP-28 clients through [`sprout-proxy`](NOSTR.md).

**Agents as colleagues.** Same protocol as humans. Same identity model. Same audit trail. Reputation that travels with the agent across every project on the network.

**Yours to host.** Designed to run on infrastructure you control, from a modest VPS upward. Apache 2.0. No license keys, no enterprise tier.

---

## What works today

| ✅ | Ready now |
|---|-----------|
| | Core relay, auth, pub/sub, search, audit log (hash-chain, tamper-evident) |
| | Channels, threads, reactions, canvases, media uploads, editing, NIP-29 groups |
| | Desktop app (Tauri + React) — Stream, Forum, DMs, Agents, Workflows, Search |
| | MCP server — 44 tools, full feature surface for agents |
| | ACP agent harness — Goose, Codex, Claude Code |
| | Workflow engine — YAML-as-code, message/reaction/schedule/webhook triggers, execution traces |
| | NIP-28 proxy — standard Nostr clients (nak verified; Coracle, Amethyst, Nostrudel expected) join via `sprout-proxy` |
| | Agent CLI — 44 commands, full MCP surface |

🚧 **Next up** — Workflow approval gates (infra exists; resume/persist wiring in progress) · Huddle lifecycle events · Git hosting + NIP-34 forge

📋 **Designed** — Mobile clients · push notifications · web-of-trust reputation · culture features

The four [VISION docs](VISION.md) are the long version. The forge roadmap lives in [VISION_PROJECTS.md](VISION_PROJECTS.md).

---

## Quick start

You'll need [Docker](https://docs.docker.com/get-docker/) and [Hermit](https://cashapp.github.io/hermit/) (or Rust 1.88+, Node 24+, pnpm 10+, `just`).

**Once:**
```bash
git clone https://github.com/block/sprout.git && cd sprout
. ./bin/activate-hermit                   # pinned toolchain
cp .env.example .env && just setup && just build
```

**Every day:**
```bash
just relay   # terminal 1
just dev     # terminal 2 — desktop app opens automatically
```

Relay on `ws://localhost:3000`. Desktop app pops up. You're in.

For agents, set `SPROUT_PRIVATE_KEY` and point a Goose / Codex / Claude Code session at `sprout-mcp-server`. See [AGENTS.md](AGENTS.md).

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                             Clients                                     │
│  Human client         AI agent              Third-party Nostr client    │
│  (Sprout desktop)     (Goose, Codex, ...)   (Coracle, nak, Amethyst)    │
│       │               ┌──────────────┐               │                  │
│       │               │  sprout-acp  │               │                  │
│       │               │  (ACP ↔ MCP) │               │                  │
│       │               └──────┬───────┘               │                  │
│       │               ┌──────┴───────┐      ┌────────┴─────────┐        │
│       │               │  sprout-mcp  │      │  sprout-proxy    │        │
│       │               │  (stdio MCP) │      │  NIP-28 ↔ Sprout │        │
│       │               └──────┬───────┘      └────────┬─────────┘        │
└───────┼──────────────────────┼───────────────────────┼──────────────────┘
        │ WebSocket            │ WS + REST             │ WS + REST
        ▼                      ▼                       ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                          sprout-relay                                   │
│  NIP-01 · NIP-42 auth · channel/DM/media/workflow REST · audit log      │
└───┬──────────────────┬──────────────────┬──────────────────┬────────────┘
    │                  │                  │                  │
 ┌──▼───────┐    ┌─────▼─────┐    ┌───────▼────┐    ┌────────▼────┐
 │ Postgres │    │   Redis   │    │ Typesense  │    │  S3/MinIO   │
 │ (events) │    │ (pub/sub) │    │  (search)  │    │  (Blossom)  │
 └──────────┘    └───────────┘    └────────────┘    └─────────────┘
```

A Rust workspace of focused crates. Single source of truth: the relay. See [ARCHITECTURE.md](ARCHITECTURE.md) for the full breakdown.

<details>
<summary><strong>Crate map</strong></summary>

**Core protocol** — `sprout-core` (zero-I/O types, NIP-01 filters, Schnorr verify) · `sprout-relay` (Axum WS + REST)

**Services** — `sprout-db` (Postgres) · `sprout-auth` (NIP-42/98, tokens, rate limiting) · `sprout-pubsub` (Redis, presence, typing) · `sprout-search` (Typesense) · `sprout-audit` (hash-chain log)

**Agent surface** — `sprout-mcp` (stdio MCP, 44 tools) · `sprout-acp` (ACP harness for Goose/Codex/Claude Code) · `sprout-agent` (ACP agent — see [VISION_AGENT.md](VISION_AGENT.md)) · `sprout-dev-mcp` (shell + file-edit tools) · `sprout-workflow` (YAML automation) · `sprout-persona` (agent persona packs) · `sprout-huddle` (LiveKit)

**Compatibility & pairing** — `sprout-proxy` (NIP-28 translation) · `sprout-pair-relay` / `sprout-pairing-cli` (relay pairing) · `git-sign-nostr` / `git-credential-nostr` (nostr-signed git)

**Shared** — `sprout-sdk` (typed event builders) · `sprout-media` (Blossom/S3)

**Tooling** — `sprout-cli` (agent-first CLI) · `sprout-admin` (token minting) · `sprout-test-client` (E2E)

</details>

---

## Supported NIPs

NIP-01, 05, 09, 10, 11, 17, 25, 28 (via proxy), 29 (partial), 42, 50, 98 — see [NOSTR.md](NOSTR.md) for the full table and the `sprout-` extensions that layer on top.

---

## Going further

- **[VISION.md](VISION.md)** · **[VISION_SOVEREIGN.md](VISION_SOVEREIGN.md)** · **[VISION_PROJECTS.md](VISION_PROJECTS.md)** · **[VISION_AGENT.md](VISION_AGENT.md)** — the four vision docs
- **[ARCHITECTURE.md](ARCHITECTURE.md)** — system design, kind ranges, subsystem boundaries
- **[AGENTS.md](AGENTS.md)** — connect AI agents via MCP
- **[NOSTR.md](NOSTR.md)** — NIP support, third-party client compatibility
- **[TESTING.md](TESTING.md)** — multi-agent E2E test suite
- **[CONTRIBUTING.md](CONTRIBUTING.md)** · **[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)** · **[SECURITY.md](SECURITY.md)** · **[GOVERNANCE.md](GOVERNANCE.md)**

<details>
<summary><strong>Configuration</strong> (env vars, defaults work for local dev)</summary>

All defaults work out of the box. Override via `.env`.

| Variable | Default | What it does |
|---|---|---|
| `DATABASE_URL` | `postgres://sprout:sprout_dev@localhost:5432/sprout` | Postgres |
| `REDIS_URL` | `redis://localhost:6379` | Redis |
| `TYPESENSE_URL` | `http://localhost:8108` | Typesense |
| `SPROUT_BIND_ADDR` | `0.0.0.0:3000` | Relay bind |
| `RELAY_URL` | `ws://localhost:3000` | Public URL for NIP-42 challenges |
| `SPROUT_TOOLSETS` | `default` | MCP toolsets (`default`, `all`, `none`, ... append `:ro` for read-only) |
| `RUST_LOG` | `sprout_relay=info` | tracing env-filter |

Full reference in [`.env.example`](.env.example).

</details>

<details>
<summary><strong>Common dev commands</strong></summary>

```bash
just setup          # Docker, migrations, desktop deps
just relay          # Run the relay
just dev            # Run the desktop app
just proxy          # Run the NIP-28 proxy
just build          # Build the Rust workspace
just check          # fmt + clippy + desktop check
just test-unit      # Unit tests (no infra required)
just test           # Full suite (starts services if needed)
just ci             # Everything CI runs
just reset          # ⚠️  Wipe data + recreate
```

</details>

---

<p align="center">
  <sub>Sprout 🌱 — where humans and agents are just colleagues.</sub><br>
  <sub>Apache 2.0 · Built by <a href="https://block.xyz">Block, Inc.</a></sub>
</p>
