# Persona Pack Specification V7

> **Status**: Draft  
> **Replaces**: PERSONA_PACK_SPEC_V6.md  
> **Last Updated**: 2026-04-10  
> **Changes from V6**:
> - **Flattened namespace**: All behavioral config fields (`model`, `temperature`, `subscribe`,
>   `respond_to`, `max_context_tokens`, `thread_replies`, `broadcast_replies`) are now **top-level
>   frontmatter fields** in `.persona.md` — the `sprout:` wrapper block is gone.
> - **`plugin.json` cleanup**: `sprout.defaults` becomes `defaults`; `sprout.personas` becomes
>   `personas`. The `sprout` wrapper object in `plugin.json` is eliminated. OPS-standard fields
>   remain at the top level as before.
> - **Generic field names for cross-project portability**: No vendor prefix required to adopt this
>   persona format. Since the Open Plugin Spec has zero model configuration, there are no field
>   name collisions.
> - All V6 corrections, architecture, precedence model logic, build requirements, and merge
>   semantics are preserved — only field references are updated.

---

## 1. Overview & Goals

A **Persona Pack** is a portable, self-contained bundle that defines one or more AI agent personas
for deployment in Sprout. It is a **superset of the [Open Plugin Spec](https://open-plugin-spec.org)**
— every valid Persona Pack is also a valid OPS package, but not vice versa.

A pack contains: personas (identity + system prompt), skills (on-demand instruction sets), MCP
server config, pack-level instructions, lifecycle hooks, and distribution metadata.

### Design Goals

1. **Portable** — zip file or git repo; no Sprout tooling required to inspect
2. **Composable** — skills and MCP servers shared across agents; per-agent overrides additive
3. **OPS-compatible** — discoverable by any OPS-compatible tool
4. **Harness-honest** — explicit about what goose does vs. what sprout-acp does
5. **Build-honest** — features requiring new sprout-acp implementation are labeled as such

### What This Spec Corrects (V1, V2, and V3 Errors)

| V1/V2/V3 Assumption | Reality | Fix |
|---------------------|---------|-----|
| `--skill-path` CLI flag exists | Does not exist in goose | Skills copied to `.agents/skills/` by harness |
| `--rules-dir` flag exists | Does not exist | Rules injected via user message prefix |
| `--system-prompt-file` flag works in ACP mode | Does not exist in goose-acp | Use `agent.extend_system_prompt()` (BR-1) |
| `.mdc` rules files supported | Goose does not read `.mdc` files | Concept eliminated |
| `skills/` at pack root is discovered | Goose scans `.agents/skills/`, not `skills/` | Skills go to `.agents/skills/` |
| Goose has a hook system | Zero hook support in goose codebase | Hooks are harness-only |
| `[System]` block injection is implemented | Not implemented in goose-acp today | Marked as BR-1 |
| Directory name is fallback when `name:` absent | **No fallback — skill silently skipped** | Both `name:` and `description:` required |
| Per-persona env vars set before subprocess | `AcpClient::spawn` inherits parent env only | Marked as BR-6 |
| SSE MCP transport works | goose-acp rejects SSE with an error | Use stdio or streamable_http |
| `$AGENT_CWD` is an env var goose reads | It's `NewSessionRequest.cwd` in the ACP protocol | Reframed throughout |

---

## 2. Open Plugin Spec Compatibility

A Persona Pack is a valid OPS package. The `.plugin/plugin.json` manifest follows the OPS schema,
and Sprout-specific extensions live alongside the OPS fields at the top level. Since the Open
Plugin Spec defines no model configuration fields, there are no collisions. OPS consumers safely
ignore unknown fields.

### `.plugin/plugin.json`

```json
{
  "$schema": "https://open-plugin-spec.org/schema/v1/plugin.json",
  "id": "com.example.meadow-security-team",
  "name": "Meadow Security Team",
  "version": "1.2.0",
  "description": "A four-agent security review team for Sprout.",
  "author": "Meadow Engineering",
  "license": "MIT",
  "homepage": "https://github.com/example/meadow-security-team",
  "keywords": ["security", "code-review", "sprout"],
  "engines": {
    "sprout": ">=0.9.0"
  },
  "personas": [
    "agents/pip.persona.md",
    "agents/lep.persona.md",
    "agents/thistle.persona.md",
    "agents/berry.persona.md"
  ],
  "pack_instructions": "instructions.md",
  "mcp_config": ".mcp.json",
  "hooks_config": "hooks/hooks.json",
  "defaults": {
    "model": "anthropic:claude-sonnet-4-20250514",
    "temperature": 0.7,
    "max_context_tokens": 128000,
    "respond_to": {
      "mentions": true,
      "keywords": [],
      "all_messages": false
    },
    "subscribe": [],
    "thread_replies": true,
    "broadcast_replies": false
  }
}
```

The `defaults` object sets pack-wide behavioral defaults for all personas. Any behavioral config
field that a persona does not explicitly set is resolved from this object. In the example above,
all four agents default to Sonnet — but `pip.persona.md` overrides with Opus:

```yaml
# agents/pip.persona.md (frontmatter excerpt)
model: "anthropic:claude-4-opus-20250514"
subscribe:
  - "#security-reviews"
```

pip gets Opus; lep, thistle, and berry get Sonnet. Temperature 0.7 applies to all four because
none of them override it.

> **Note**: `subscribe` and `respond_to` in `defaults` are valid but unusual — most packs set
> these per-persona since agents typically monitor different channels and respond to different
> triggers.

### Compatibility Rules

- **OPS consumers**: see standard metadata; safely ignore unknown fields including `personas`,
  `defaults`, `pack_instructions`, `mcp_config`, and `hooks_config`.
- **Sprout**: reads both OPS fields and the Sprout-specific fields; `personas` is authoritative.
- **Version negotiation**: `engines.sprout` specifies minimum required Sprout version; sprout-acp
  rejects packs requiring a newer version.
- **Extension mechanism**: Sprout-specific fields sit at the top level of `plugin.json` alongside
  OPS fields. No OPS core field is overloaded.
- **`defaults`**: ignored entirely by OPS consumers. sprout-acp resolves it at deploy time before
  constructing per-persona configurations (see Section 9 and Section 11).

---

## 3. Pack Layout

```
my-pack/
├── .plugin/
│   └── plugin.json          # OPS manifest (superset)
├── agents/
│   ├── pip.persona.md        # Persona: identity + system prompt
│   ├── lep.persona.md
│   ├── thistle.persona.md
│   └── berry.persona.md
├── skills/                   # Pack skills (harness copies to .agents/skills/)
│   ├── code-review/
│   │   └── SKILL.md
│   ├── security-review/
│   │   └── SKILL.md
│   └── shared/
│       └── SKILL.md
├── .mcp.json                 # Pack-level MCP server config (shared)
├── hooks/
│   └── hooks.json            # Lifecycle hooks (harness-managed, NOT goose)
├── instructions.md           # Pack-level instructions (injected by harness)
├── pack.lock                 # Version lock (Phase 1+)
├── README.md                 # Human-readable description
└── my-pack-1.2.0.sproutpack.sha256  # Checksum (required for zip distribution)
```

### Directory Conventions

- `agents/` — all persona files. No nesting; flat directory.
- `skills/` — one subdirectory per skill. Each skill directory contains a `SKILL.md` file.
  Both `name:` and `description:` frontmatter fields are **required** — see Section 5.
- `.plugin/` — OPS-required location for the manifest.
- `hooks/` — optional; omit if no hooks are needed.
- `instructions.md` — optional; omit if no pack-level instructions.
- `.mcp.json` — optional; omit if no shared MCP servers.

Pack contents must not include: agent working directory state (`.goose/`, `.agents/`, etc.),
secrets or API keys (use `${VAR_NAME}` references), or build artifacts.

---

## 4. Persona File Format (`.persona.md`)

A persona file is a markdown document with YAML frontmatter. The **YAML frontmatter** defines
identity, skills, MCP servers, and behavioral config. The **markdown body** (everything after the
closing `---`) is the agent's persona prompt text.

> **Build Requirement**: Delivery of the persona prompt as a true goose system prompt requires
> sprout-acp to call `agent.extend_system_prompt()` (or `agent.override_system_prompt()`) after
> `create_agent_for_session()` in `on_new_session()`. These methods exist in
> `crates/goose/src/agents/agent.rs` but are **not currently called from the ACP path**. Until
> this is implemented, sprout-acp prepends the persona prompt as a `[System]` prefix in the user
> message text (see Section 11). See Section 15 for the full build requirements list.

### Full Schema

```yaml
---
# === Identity ===
name: "lep"
display_name: "Lep 🍀"
avatar: "./avatars/lep.png"
description: "Security-focused code reviewer"

# === Open Plugin Spec fields ===
version: "1.0.0"
author: "Meadow Team"

# === Skills ===
skills:
  - "./skills/security-review/"
  - "./skills/code-review/"

# === MCP Servers (per-persona) ===
mcp_servers:
  - name: "semgrep"
    command: "semgrep-mcp"
    args: ["--stdio"]
    env:
      SEMGREP_TOKEN: "${SEMGREP_TOKEN}"

# === Behavioral Config (Sprout-specific) ===
subscribe:
  - "#security-reviews"
  - "#code-reviews"
respond_to:
  mentions: true
  keywords: ["security", "vulnerability", "CVE"]
model: "anthropic:claude-sonnet-4-20250514"
temperature: 0.3
max_context_tokens: 128000

# === Hooks (harness-managed, NOT goose) ===
hooks:
  on_start: "./hooks/setup-semgrep.sh"
  on_stop: "./hooks/cleanup.sh"
  on_message: null
---

You are Lep, a security-focused code reviewer on the Meadow team.
...
```

### Field Reference

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | ✅ | Machine name / agent ID. Lowercase, no spaces, unique within pack. |
| `display_name` | string | ✅ | Human-readable name shown in Sprout UI. |
| `avatar` | string | ❌ | Pack-relative path to avatar image. |
| `description` | string | ✅ | One-line description. |
| `version` | string | ❌ | Semver. Defaults to pack version if omitted. |
| `author` | string | ❌ | OPS compatibility field. |
| `skills` | string[] | ❌ | Pack-relative paths to skill directories for this agent only. |
| `mcp_servers` | object[] | ❌ | Per-persona MCP servers. Merged with pack-level `.mcp.json`. |
| `subscribe` | string[] | ❌ | Channels to monitor. See Section 9. |
| `respond_to` | object | ❌ | Controls which messages trigger a response. See Section 9. |
| `model` | string | ❌ | Model to use. See Section 9. |
| `temperature` | float | ❌ | Sampling temperature. See Section 9. |
| `max_context_tokens` | int | ❌ | Context window limit. See Section 9. |
| `thread_replies` | bool | ❌ | Reply in-thread when triggering message is in a thread. See Section 9. |
| `broadcast_replies` | bool | ❌ | Surface thread replies to the main channel. See Section 9. |
| `hooks` | object | ❌ | Lifecycle hooks. Harness-managed. See Section 8. |

### Markdown Body (Persona Prompt)

Everything after the closing `---` is the persona prompt text. Pack-level `instructions.md` is
appended after it. Embed the prompt directly — do not reference external files, `--system-prompt-file`
(does not exist in goose-acp), or `.mdc` rule files (goose does not read them).

---

## 5. Skills

Skills are reusable instruction sets that agents load on demand. They are markdown files that teach
the agent how to perform a specific task.

### Discovery

Goose discovers skills from these directories relative to the session working directory (`$AGENT_CWD`
— see definition below):

```
$AGENT_CWD/.goose/skills/<skill-name>/SKILL.md
$AGENT_CWD/.claude/skills/<skill-name>/SKILL.md
$AGENT_CWD/.agents/skills/<skill-name>/SKILL.md   ← sprout-acp uses this one
```

> **Note**: `$AGENT_CWD/skills/` is NOT scanned. Skills placed at the pack root `skills/` directory
> are not discoverable by goose until the harness copies them.

### `$AGENT_CWD` Definition

Throughout this spec, **`$AGENT_CWD`** refers to the `cwd` field in the ACP `NewSessionRequest`
— the working directory passed to goose-acp when creating a session. This is an ACP protocol
field, not an environment variable that goose reads at runtime.

sprout-acp determines what value to pass as `NewSessionRequest.cwd` in this order:

1. The `AGENT_CWD` environment variable, if set.
2. `std::env::current_dir()` as a fallback.
3. If both fail, sprout-acp logs an error and **refuses to start**.

goose-acp stores this value as `session.working_dir` and uses it for all skill discovery.

### Skill Name Resolution (Load Key)

The load key used in `load(source: "skill-name")` is the `name:` field from `SKILL.md` frontmatter.

**Both `name:` and `description:` are required fields in `SKILL.md` frontmatter.** The goose
codebase defines:

```rust
#[derive(Debug, Deserialize)]
struct SkillMetadata {
    name: String,
    description: String,
}
```

If either field is absent or the frontmatter is malformed, `parse_frontmatter` returns `None` and
the skill is **silently skipped**. There is **no fallback to the directory name**.

> **Recommendation**: Use the directory name as the `name:` value for consistency (e.g., a skill
> in `skills/security-review/` should have `name: "security-review"`). This avoids load key
> mismatches and makes `load(source: "security-review")` predictable.

### Skill Scoping Rules

Skills in the pack's `skills/` directory are copied to agent working directories according to these rules:

| Condition | Destination |
|-----------|-------------|
| Skill directory is listed in **at least one** persona's `skills:` array | Copied **only** to that persona's `$AGENT_CWD/.agents/skills/` |
| Skill directory is **not listed in any** persona's `skills:` array | Copied to **all** agents' `$AGENT_CWD/.agents/skills/` |

**Key implication**: Once a skill is claimed by any persona, it is no longer automatically shared
with other agents. If you want a skill available to all agents AND explicitly listed in one persona's
`skills:` array, list it in every persona's `skills:` array.

### Collision Handling

If a skill with the same load key already exists in `$AGENT_CWD/.agents/skills/`, the pack skill
is **not overwritten**. This allows operators to pin custom skill versions. sprout-acp **must log
a warning** when a pack skill is skipped due to a collision:

```
WARN: Skill "security-review" already exists at .agents/skills/security-review/; skipping pack version
```

### Loading

Skills are not auto-loaded into context. The agent must explicitly load them:

```
load(source: "security-review")
```

sprout-acp lists available skills in the user message prefix so the agent knows what's available.
See Section 11 for the full message format.

### Skill File Format

```markdown
---
name: "security-review"
description: "Reviews code for security vulnerabilities using OWASP Top 10 and semgrep"
---

# Security Review

...content...
```

Both `name:` and `description:` are **required**. A skill missing either field is silently skipped
by goose. `sprout pack validate` (BR-2) must flag missing fields as errors.

---

## 6. MCP Server Configuration

MCP servers provide external tool access (GitHub, Semgrep, databases, etc.). Configuration is
defined at two levels: pack-level (shared across all agents) and per-persona (agent-specific).
sprout-acp merges them and passes the result via the ACP protocol — no filesystem placement required.

> **Transport Warning**: Only `stdio` and `streamable_http` transports are supported. SSE transport
> is rejected by goose-acp with the error `"SSE is unsupported, migrate to streamable_http"` and
> will cause session startup to fail. Migrate any SSE-based MCP servers to streamable_http before
> packaging.

### Pack-Level: `.mcp.json`

```json
{
  "mcpServers": {
    "github": {
      "command": "github-mcp-server",
      "args": ["stdio"],
      "env": {
        "GITHUB_PERSONAL_ACCESS_TOKEN": "${GITHUB_TOKEN}"
      }
    },
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/workspace"]
    }
  }
}
```

### Per-Persona: `mcp_servers` in Frontmatter

```yaml
mcp_servers:
  - name: "semgrep"
    command: "semgrep-mcp"
    args: ["--stdio"]
    env:
      SEMGREP_TOKEN: "${SEMGREP_TOKEN}"
```

### Merge Rules

1. Pack-level servers are the base set; per-persona servers merged on top.
2. **Name collision**: per-persona entry wins entirely (no partial merge).
3. The merged set is passed to goose-acp via `NewSessionRequest.mcp_servers`.

### Environment Variable Interpolation

All `env` values are scanned for `${VAR_NAME}`. sprout-acp resolves from the process environment
**before** passing to goose-acp. Unresolved variables cause a startup error:

```
Error: MCP server "semgrep" requires env var SEMGREP_TOKEN (not set)
```

### Delivery

sprout-acp passes the merged config via `NewSessionRequest.mcp_servers` (fully wired in goose-acp,
`server.rs`). **No `.mcp.json` is written to the agent's working directory.**

---

## 7. Pack-Level Instructions

`instructions.md` contains shared rules, coding standards, and team norms that apply to all agents
in the pack. sprout-acp appends it to the persona prompt in the user message prefix.

sprout-acp appends `instructions.md` to the persona prompt in the user message prefix (see
Section 11). **No file is written to disk.**

**What does NOT work**: `.mdc` rule files (goose doesn't read them), `rules/` directory (no
`--rules-dir` flag), relying on the pack's `AGENTS.md` for runtime injection (it's for human
contributors only).

> **Note**: Goose auto-loads `AGENTS.md` and `.goosehints` from `$AGENT_CWD` (walking up to git
> root). Operators can place instructions there as a secondary mechanism, but the canonical path
> is harness injection via the user message prefix.

---

## 8. Lifecycle Hooks

Hooks are shell commands fired by sprout-acp at agent lifecycle points. **Goose has no hook system**
— hooks are entirely a harness feature.

### `hooks/hooks.json`

Pack-level hooks apply to all agents:

```json
{
  "on_start": "./hooks/setup.sh",
  "on_stop": "./hooks/cleanup.sh",
  "on_message": null
}
```

### Per-Persona Hooks

Per-persona hooks override pack-level hooks for that agent:

```yaml
hooks:
  on_start: "./hooks/setup-semgrep.sh"
  on_stop: "./hooks/cleanup.sh"
  on_message: null
```

### Hook Points

| Hook | When Fired | Use Cases |
|------|-----------|-----------|
| `on_start` | Before the agent session starts | Install dependencies, warm caches, validate credentials |
| `on_stop` | After the agent session ends (normal exit or error) | Cleanup temp files, flush logs, release locks |
| `on_message` | Before each message is dispatched to the agent | Rate limiting, logging, message preprocessing |

### Hook Execution

Hooks run as the sprout-acp user; working directory is `$AGENT_CWD`; agent env vars are available.
Exit codes: `on_start` non-zero → abort startup; `on_stop` non-zero → logged only; `on_message`
non-zero → message dropped and error logged.

### `on_message` Hook Contract

The `on_message` hook receives the incoming message content via **stdin** (UTF-8 text). It is a
**read-only side-effect hook** — it cannot modify the message. If you need message transformation,
that must be implemented directly in sprout-acp's dispatch loop, not via a hook.

- **Timeout**: 5 seconds. Hooks that exceed this are killed (SIGKILL) and the message is dropped.
- **Non-zero exit**: Message is dropped and an error is logged. The agent does not see the message.
- **Stdout/stderr**: Captured and logged at DEBUG level. Not passed to the agent.

### `on_stop` Crash Caveat

`on_stop` fires on normal exit and on handled errors. It **will not fire** if sprout-acp crashes
(SIGSEGV, OOM, etc.). For critical cleanup (lock files, external resource release), use a
systemd/supervisor cleanup unit or a process supervisor that runs cleanup unconditionally.

**Hooks are NOT goose features.** They are implemented entirely in sprout-acp. Bypassing
sprout-acp means no hooks fire.

---

## 9. Behavioral Configuration

The behavioral config fields in a persona's frontmatter control how the agent participates in
Sprout conversations. These are all Sprout-specific — goose has no awareness of them. They sit
at the top level of the frontmatter alongside identity fields like `name` and `description`.

### Pack Defaults

Teams of four or more agents often share the same model, temperature, and response settings. The
`defaults` object in `plugin.json` sets pack-wide values for all behavioral config fields.
Per-persona frontmatter fields override them.

If `plugin.json` does not contain a `defaults` key, level 4 is skipped entirely and fields fall
through directly to built-in defaults (level 5).

**Example**: A four-agent security team where all agents use Sonnet except the orchestrator (pip),
which uses Opus.

`plugin.json`:
```json
{
  "personas": [
    "agents/pip.persona.md",
    "agents/lep.persona.md",
    "agents/thistle.persona.md",
    "agents/berry.persona.md"
  ],
  "defaults": {
    "model": "anthropic:claude-sonnet-4-20250514",
    "temperature": 0.7,
    "max_context_tokens": 128000,
    "respond_to": {
      "mentions": true,
      "keywords": [],
      "all_messages": false
    },
    "subscribe": [],
    "thread_replies": true,
    "broadcast_replies": false
  }
}
```

> **Note**: `subscribe` and `respond_to` in `defaults` are valid but unusual — most packs set
> these per-persona since agents typically monitor different channels and respond to different
> triggers.

`agents/pip.persona.md` (frontmatter excerpt):
```yaml
model: "anthropic:claude-4-opus-20250514"
subscribe:
  - "#security-reviews"
```

Result:
- **pip**: model=Opus, temperature=0.7 (from pack default), max_context_tokens=128000 (from pack default)
- **lep, thistle, berry**: model=Sonnet, temperature=0.7, max_context_tokens=128000 (all from pack default)

### Precedence Model

In this spec, **"deploy time"** means when sprout-acp loads the pack and constructs per-persona
session configurations — typically at sprout-acp process startup. For git-based packs, this occurs
each time sprout-acp starts and reads the installed pack directory.

When sprout-acp resolves the effective configuration for a persona, it applies this order (highest
wins):

```
1. Operator env vars           — GOOSE_MODEL, GOOSE_PROVIDER, GOOSE_TEMPERATURE, GOOSE_CONTEXT_LIMIT
                                 already set in the parent process environment
2. Desktop UI per-agent        — overrides set in the Sprout desktop app per-agent settings
3. Per-persona frontmatter     — behavioral config fields set directly in the persona's frontmatter
4. Pack-level defaults         — the `defaults` object in plugin.json
5. Built-in defaults           — sprout-acp's hardcoded fallback values
```

sprout-acp resolves levels 3–5 at deploy time (when the pack is loaded and sessions are
constructed). Levels 1–2 are applied at runtime and are outside the pack's control.

**Level 1 — Operator env vars**: If `GOOSE_MODEL`, `GOOSE_PROVIDER`, `GOOSE_TEMPERATURE`, or
`GOOSE_CONTEXT_LIMIT` are already set in the parent process environment before sprout-acp starts,
sprout-acp MUST NOT override them with pack/persona values. sprout-acp only injects env vars for
fields that are NOT already set in the parent environment. This ensures operators can always
override pack configuration by setting env vars on the sprout-acp process.

**Implementation**: when constructing the child process environment (BR-6), sprout-acp checks
`std::env::var(key)` for each env var. If the parent already has it set, skip injection. If not,
inject the resolved pack/persona value.

### Empty and `null` Semantics

The following rules govern how absent, empty, and null values are interpreted in a persona's
behavioral config frontmatter fields:

- **All behavioral config fields absent** (no `model`, `temperature`, `subscribe`, etc.) is
  equivalent to having no overrides — all pack defaults apply.

- **`temperature: null`** — `null` values are treated as absent. The field falls through to the
  next precedence level (pack default, then built-in default). This allows a persona to explicitly
  "unset" a field it previously set.

- **`subscribe: []`** — an empty array is NOT treated as absent. It means "subscribe to nothing."
  This is an intentional override that prevents pack defaults from applying.

- **`respond_to: {}`** — an empty object is NOT treated as absent. It means "use default sub-field
  values." This overrides the pack default `respond_to` object entirely, and each sub-field falls
  through to its built-in default.

> **Rule of thumb**: `null` = absent (fall through). Empty containers (`[]`, `{}`) = present
> (override).

### Merge Semantics

Field merging is **shallow replacement** — there is no deep merge. The rules are:

- **Simple fields** (`model`, `temperature`, `max_context_tokens`, `thread_replies`,
  `broadcast_replies`): the first defined value in the precedence chain wins entirely.
- **Object fields** (`respond_to`): if the persona sets `respond_to`, the entire object replaces
  the pack default. Individual sub-keys are not merged. If the persona does not set `respond_to`,
  the pack default `respond_to` object is used as-is.
- **Array fields** (`subscribe`): if the persona sets `subscribe`, the entire array replaces the
  pack default. There is no union or append behavior. If the persona does not set `subscribe`, the
  pack default `subscribe` array is used as-is.

**Example — object replacement**:

Pack default (`defaults` in `plugin.json`):
```json
"respond_to": { "mentions": true, "keywords": ["security"], "all_messages": false }
```

Persona override (frontmatter):
```yaml
respond_to:
  mentions: true
  all_messages: true
```

Effective result for that persona:
```json
{ "mentions": true, "all_messages": true }
```

Note: `keywords` is **gone** — the persona's `respond_to` replaced the entire object. There is no
implicit inheritance of sub-keys.

**Example — array replacement**:

Pack default (`defaults` in `plugin.json`):
```json
"subscribe": ["#general"]
```

Persona override (frontmatter):
```yaml
subscribe:
  - "#security-reviews"
  - "#code-reviews"
```

Effective result: `["#security-reviews", "#code-reviews"]` — `#general` is not included.

**Example — empty object override**:

Pack default (`defaults` in `plugin.json`):
```json
"respond_to": { "mentions": true, "keywords": ["security"], "all_messages": false }
```

Persona override (frontmatter):
```yaml
respond_to: {}
```

Effective result for that persona:
```json
{ "mentions": true, "keywords": [], "all_messages": false }
```

Note: `respond_to: {}` is NOT absent — it overrides the pack default entirely. Each sub-field
falls through to its **built-in default** (not the pack default). `mentions` defaults to `true`,
`keywords` to `[]`, `all_messages` to `false`.

### Canonical Behavioral Config Field Schema

This schema applies identically to both the `defaults` object in `plugin.json` and the top-level
behavioral config fields in `.persona.md` frontmatter. The same keys, types, and validation rules
apply to both.

| Field | Type | Built-in Default | Valid Range / Values | Description |
|-------|------|-----------------|----------------------|-------------|
| `subscribe` | string[] | `[]` | Any channel name strings | Channels to monitor. `#` prefix stripped before relay calls. |
| `respond_to` | object | see sub-fields | — | Controls which messages trigger a response. Replaced as a whole unit on override. |
| `respond_to.mentions` | bool | `true` | `true` / `false` | Respond when @mentioned. |
| `respond_to.keywords` | string[] | `[]` | Any strings | Respond when message contains any keyword (case-insensitive). |
| `respond_to.all_messages` | bool | `false` | `true` / `false` | Respond to every message in subscribed channels. |
| `model` | string | none (goose uses operator default) | `"provider:model-id"` format | Model to use. Split on first `:` → `GOOSE_PROVIDER` + `GOOSE_MODEL`. |
| `temperature` | float | `0.7` | Provider-dependent (typically 0.0–2.0). sprout-acp passes through without range validation; `sprout pack validate` checks type only (must be a number), not range. | Sampling temperature → `GOOSE_TEMPERATURE`. |
| `max_context_tokens` | int | none (provider default) | Positive integer | Context window limit → `GOOSE_CONTEXT_LIMIT`. |
| `thread_replies` | bool | `true` | `true` / `false` | Reply in-thread when the triggering message is in a thread. |
| `broadcast_replies` | bool | `false` | `true` / `false` | Also surface thread replies to the main channel. |

**Unknown keys** in `defaults` (in `plugin.json`) or in a persona's frontmatter behavioral config
fields are **validation errors** in `sprout pack validate` (BR-2) — this catches typos like
`temprature` at validate time. At deploy time, sprout-acp logs a `WARN` and ignores the unknown
key, remaining fail-soft:

```
WARN: Unknown key "temprature" in defaults (plugin.json); ignoring
```

### Full Behavioral Config Reference

```yaml
# In a .persona.md frontmatter — behavioral config fields at top level:

subscribe:
  - "#security-reviews"
  - "#code-reviews"

respond_to:
  mentions: true
  keywords:
    - "security"
    - "vulnerability"
    - "CVE"
  all_messages: false

model: "anthropic:claude-sonnet-4-20250514"
temperature: 0.3
max_context_tokens: 128000

thread_replies: true
broadcast_replies: false
```

### Channel Name `#` Convention

The `#` prefix in `subscribe` entries is a **display convention only**. Channel names in the Sprout
relay are stored and queried **without** the `#` prefix. sprout-acp strips the leading `#` before
making any relay API calls. `"#security-reviews"` and `"security-reviews"` are equivalent in this
field.

### Env Var Mapping

sprout-acp resolves pack defaults and per-persona overrides (precedence levels 3–5) into a single
effective configuration per persona **before** injecting environment variables into the child
process. The env vars set reflect the fully-resolved values — not the raw persona frontmatter.

sprout-acp translates persona behavioral config fields to goose configuration via environment
variables injected into the child process at spawn time (requires BR-6):

| Persona field | Env var(s) | Notes |
|---|---|---|
| `model: "anthropic:claude-sonnet-4-20250514"` | `GOOSE_PROVIDER=anthropic` + `GOOSE_MODEL=claude-sonnet-4-20250514` | Split on first `:` |
| `temperature: 0.3` | `GOOSE_TEMPERATURE=0.3` | Read via `std::env::var` at model creation |
| `max_context_tokens: 128000` | `GOOSE_CONTEXT_LIMIT=128000` | Read via `Config::global().get_param()` — cached at first use |

If `model` is omitted from both the persona frontmatter and `defaults`, sprout-acp does not set
`GOOSE_PROVIDER` or `GOOSE_MODEL`, and goose uses its configured operator default.

> **Current limitation**: `AcpClient::spawn` (in `crates/sprout-acp/src/acp.rs`) inherits the
> parent process environment — there are no `.env()` calls. In a multi-persona deployment, env
> vars set on the parent affect **all** spawned agents. Per-persona model config requires BR-6.
> Until then, per-persona model configuration is only safe with one sprout-acp process per persona.
>
> **Alternative**: goose-acp implements `session/set_model` (`on_set_model()` in `server.rs`,
> ACP unstable). This could set the model per-session after `session/new` without env var
> injection — but only covers model, not provider/temperature/context. See open question 2.

See the Canonical Behavioral Config Field Schema table above for the full field reference.

> **Built-in defaults note**: The "Built-in Default" column in the Canonical Behavioral Config
> Field Schema table lists sprout-acp's built-in fallbacks (precedence level 5). If `defaults` is
> present in `plugin.json`, those values take precedence over the built-in defaults (level 4 >
> level 5). The built-in defaults only apply when neither the persona nor the pack defaults specify
> a value.

All fields are consumed entirely by sprout-acp. None are passed to goose directly.

---

## 10. Distribution

### Phase 1: Zip File

A pack is distributed as a `.sproutpack` file (zip archive):

```bash
sprout pack validate ./my-pack
sprout pack ./my-pack --output my-pack-1.2.0.sproutpack
sprout install ./my-pack-1.2.0.sproutpack
sprout install https://example.com/releases/my-pack-1.2.0.sproutpack
```

#### Pack Integrity (Required)

Zip packs **must** ship with `<pack-name>-<version>.sproutpack.sha256` containing `sha256sum`
output (`<hex-digest>  <filename>`). sprout-acp **must** verify before installation and refuse on
mismatch. For HTTP installs, the checksum file is fetched from the same base URL.

#### `pack.lock` for Phase 1

Phase 1 installs record the installed pack in `pack.lock` alongside the pack directory:

```json
{
  "com.example.meadow-security-team": {
    "source": "https://example.com/releases/my-pack-1.2.0.sproutpack",
    "sha256": "a3f1c2d4e5b6...",
    "version": "1.2.0",
    "installed_at": "2026-04-10T11:00:00Z"
  }
}
```

### Phase 2: Git Repository

```bash
sprout install github:example/meadow-security-team
sprout install github:example/meadow-security-team@v1.2.0
sprout install git+https://gitlab.example.com/team/pack.git
```

`pack.lock` for git installs records the resolved commit SHA:

```json
{
  "com.example.meadow-security-team": {
    "source": "github:example/meadow-security-team",
    "resolved": "github:example/meadow-security-team#abc1234",
    "version": "1.2.0",
    "installed_at": "2026-04-10T11:00:00Z"
  }
}
```

### Phase 3: App Store UI

A Sprout-hosted registry and in-app browser for discovering and installing packs. API-compatible
with OPS registries. Details TBD.

### Installed Pack Location

Installed packs live at `~/.sprout/packs/<pack-id>/`. sprout-acp reads packs from this location
at agent startup.

---

## 11. Delivery Mechanism Summary

How each pack component reaches the running agent:

| Component | Delivery Method | Mechanism | Filesystem Write? |
|-----------|----------------|-----------|-------------------|
| Skills | Copy at deploy time | sprout-acp copies `skills/` → `$AGENT_CWD/.agents/skills/` | ✅ Yes (only one) |
| MCP servers | ACP protocol | `NewSessionRequest.mcp_servers` | ❌ No |
| Persona prompt | User message prefix | `[System]` block prepended to user message text by sprout-acp | ❌ No |
| Pack instructions | User message prefix | Appended to `[System]` block in user message text | ❌ No |
| Lifecycle hooks | Harness internal | sprout-acp fires shell commands directly | ❌ No |
| Model/provider | Child process env vars | `GOOSE_PROVIDER`, `GOOSE_MODEL`, `GOOSE_TEMPERATURE`, `GOOSE_CONTEXT_LIMIT` (requires BR-6) | ❌ No |
| Behavioral config | Harness internal | sprout-acp subscription + dispatch logic | ❌ No |
| Pack defaults (`defaults`) | Harness internal | Resolved at deploy time by sprout-acp into per-persona effective config; never passed to goose directly | ❌ No |

> **Pack defaults are resolved at deploy time**, not at runtime. When sprout-acp loads a pack and
> constructs per-persona session configurations, it merges the `defaults` object with each persona's
> frontmatter behavioral config fields (per the precedence model in Section 9) and stores the
> resulting effective configuration. The `defaults` object itself is not forwarded to goose or
> stored in any runtime artifact — only the resolved per-persona values are used.

### The `[System]` Block — Current Implementation

sprout-acp's `format_prompt()` in `queue.rs` prepends a `[System]` block to the **user message
text** before sending it to goose. This is a **sprout-acp feature, not a goose feature**. Goose
sees the `[System]` prefix as part of the user message content — it is NOT injected into goose's
actual system prompt.

For persona-backed agents, the `[System]` block contains:

```
[System]
<persona prompt (markdown body of .persona.md)>

---
# Team Instructions
<contents of instructions.md, if present>

---
Available skills: code-review, security-review, shared
Load a skill with: load(source: "skill-name")
```

### True System Prompt Injection — Build Requirement (BR-1)

The `[System]` prefix re-sends the full persona prompt on every turn. True system prompt injection
— calling `agent.extend_system_prompt()` after `create_agent_for_session()` in `on_new_session()`
— fires once at session creation. See BR-1 in Section 15.

### What Does NOT Work (Anti-Pattern Reference)

| Anti-Pattern | Why It Fails |
|-------------|-------------|
| `goose acp --skill-path ./skills` | `--skill-path` flag does not exist in goose |
| `goose acp --rules-dir ./rules` | `--rules-dir` flag does not exist in goose |
| `goose acp --system-prompt-file ./prompt.md` | Flag does not exist in goose-acp |
| `rules/*.mdc` files | Goose does not read `.mdc` files |
| `skills/` at pack root (without copying) | Goose scans `.agents/skills/`, not `skills/` |
| Hooks in goose config | Goose has no hook system |
| SSE transport in `.mcp.json` | goose-acp rejects SSE; use stdio or streamable_http |
| SKILL.md without `name:` or `description:` | Skill silently skipped; no fallback |
| Setting `GOOSE_MODEL` on parent process (multi-persona) | Affects all agents; use per-subprocess injection (BR-6) |
| Expecting `defaults` sub-key inheritance | No deep merge; object/array fields replaced entirely |

---

## 12. Security Considerations

### Secret Management

Never embed secrets in pack files. Use `${VAR_NAME}` references in all `env` blocks. sprout-acp
resolves them from the process environment at startup and refuses to start if any are unresolved.
Inject secrets via your deployment mechanism (systemd env files, Vault, Kubernetes secrets, etc.).

### Pack Integrity

- **Phase 1 (zip)**: Packs **must** ship with `<pack-name>-<version>.sproutpack.sha256` containing
  `sha256sum` output (`<hex-digest>  <filename>`). sprout-acp **must** verify before installation
  and refuse on mismatch.
- **Phase 2 (git)**: `pack.lock` pins the resolved commit SHA; sprout-acp verifies on install.
- **Phase 3 (registry)**: Registry signatures TBD.

### Hook Execution

Hooks run with sprout-acp's privileges — significant attack surface. Only install packs from
trusted sources. Review all hook commands before installing. Consider sandboxing sprout-acp
(container, restricted user) for untrusted packs. sprout-acp should display hook commands before
first execution (Phase 2 feature).

### MCP Server and Skill Trust

MCP servers are external processes with tool access; audit all configs before deploying. Skills
are markdown injected into agent context; malicious content can attempt prompt injection. Treat
both with the same caution as any untrusted prompt content.

---

## 13. Migration Path

### From V6 (sprout-namespaced) Format

Field mapping from V6 `.persona.md` to V7 `.persona.md`:

| V6 location | V7 location |
|---|---|
| `sprout.model` | `model` (top-level frontmatter) |
| `sprout.temperature` | `temperature` (top-level frontmatter) |
| `sprout.max_context_tokens` | `max_context_tokens` (top-level frontmatter) |
| `sprout.subscribe` | `subscribe` (top-level frontmatter) |
| `sprout.respond_to` | `respond_to` (top-level frontmatter) |
| `sprout.thread_replies` | `thread_replies` (top-level frontmatter) |
| `sprout.broadcast_replies` | `broadcast_replies` (top-level frontmatter) |
| `plugin.json` → `sprout.defaults` | `plugin.json` → `defaults` (top-level) |
| `plugin.json` → `sprout.personas` | `plugin.json` → `personas` (top-level) |
| `plugin.json` → `sprout.pack_instructions` | `plugin.json` → `pack_instructions` (top-level) |
| `plugin.json` → `sprout.mcp_config` | `plugin.json` → `mcp_config` (top-level) |
| `plugin.json` → `sprout.hooks_config` | `plugin.json` → `hooks_config` (top-level) |

**V6 persona frontmatter** (before):
```yaml
sprout:
  model: "anthropic:claude-sonnet-4-20250514"
  temperature: 0.3
  subscribe:
    - "#security-reviews"
```

**V7 persona frontmatter** (after):
```yaml
model: "anthropic:claude-sonnet-4-20250514"
temperature: 0.3
subscribe:
  - "#security-reviews"
```

**V6 `plugin.json`** (before):
```json
"sprout": {
  "personas": ["agents/pip.persona.md"],
  "defaults": { "model": "anthropic:claude-sonnet-4-20250514" }
}
```

**V7 `plugin.json`** (after):
```json
"personas": ["agents/pip.persona.md"],
"defaults": { "model": "anthropic:claude-sonnet-4-20250514" }
```

### From Pre-V6 JSON Persona Format

Field mapping from flat JSON (`personas/lep.json`) to `.persona.md`:

| JSON field | `.persona.md` location |
|---|---|
| `system_prompt` | Markdown body (after closing `---`) |
| `model` | `model` (top-level frontmatter) |
| `channels` | `subscribe` (top-level frontmatter) |
| `mcp_servers` | Frontmatter `mcp_servers:` or pack-level `.mcp.json` |
| All other fields | Frontmatter (same names) |

### Migration Steps

1. Create pack directory with `.plugin/plugin.json`
2. For each persona JSON → create `agents/<name>.persona.md` using the mapping above
3. Move skills to `skills/<skill-name>/SKILL.md`; ensure each has `name:` and `description:` frontmatter
4. Create `instructions.md` from any shared prompt content
5. Run `sprout pack validate ./my-pack`

### Backward Compatibility

sprout-acp supports both V6 (namespaced `sprout:` block) and V7 (flat top-level fields) formats
during the transition period. The V6 namespaced format is deprecated as of the version that ships
this spec and will be removed in the next major version.

---

## 14. Open Questions / Future Work

### Closed Questions

1. ~~**`--system-prompt-file` in ACP mode**~~: **Closed.** The flag does not exist in goose-acp.
   sprout-acp uses `agent.extend_system_prompt()` instead (see BR-1).

### Unresolved

2. **`session/set_model` as env var alternative**: goose-acp implements `on_set_model()` (ACP
   unstable feature). sprout-acp could call `session/set_model` after `session/new` to set the
   model per-session without env var injection. This avoids the `AcpClient::spawn` limitation for
   model (but not provider, temperature, or context limit). Deferred pending stability of the ACP
   unstable feature.

3. **`CONTEXT_FILE_NAMES` env var**: Goose supports this env var to control which filenames are
   scanned for hints. Should sprout-acp set this to include pack-specific filenames? Deferred
   pending use case.

4. **Skill versioning**: Skills are identified by load key only. If two packs provide a skill with
   the same name, the no-overwrite rule means the first-installed wins silently. A versioned skill
   format (e.g., `code-review@1.2.0`) would resolve this.

5. **Pack signing**: Phase 3 registry needs a signing scheme. Ed25519 keypairs tied to pack author
   identity is the likely approach, but not yet designed.

6. **Multi-pack conflicts**: What happens when two installed packs define agents that subscribe to
   the same channel with overlapping `respond_to` rules? Need a conflict resolution policy.

### Future Work

`sprout pack init` scaffolding; hot reload of skills/instructions; skill marketplace; pack dependencies; agent-to-agent handoff within a pack.

---

## 15. Build Requirements

Features required by this spec but not yet implemented. These are implementation tasks.

| ID | What | Where |
|----|------|-------|
| BR-1 | True system prompt injection: call `agent.extend_system_prompt()` after `create_agent_for_session()` in `on_new_session()`. Current `[System]` prefix re-sends persona prompt on every turn; true injection fires once at session creation. | `goose/src/agents/agent.rs` + `goose-acp/src/server.rs` `on_new_session()` |
| BR-2 | `sprout pack validate` CLI: schema-validate `plugin.json` (including `defaults` — unknown keys are errors, type mismatches are errors); check `.persona.md` required identity fields; validate per-persona behavioral config fields against the canonical schema; verify all `skills:` and `hooks:` paths exist; error on `SKILL.md` missing `name:` or `description:`; warn when `name:` differs from directory name. | `sprout-cli` / `sprout-admin` |
| BR-3 | Skill collision warning: emit `WARN` when a pack skill is skipped because a skill with the same load key already exists in `.agents/skills/`. | sprout-acp skill copy logic |
| BR-4 | `$AGENT_CWD` resolution: determine `NewSessionRequest.cwd` from (1) `AGENT_CWD` env var, (2) `std::env::current_dir()`, (3) error and refuse to start. | sprout-acp startup / session init |
| BR-5 | Skill parse failure warning: emit `WARN` when `parse_skill_content` returns `None` (missing `name:`, missing `description:`, or malformed frontmatter). Currently goose silently skips. Until fixed in goose, sprout-acp should pre-validate during skill copy. | `goose/src/agents/platform_extensions/summon.rs` `parse_skill_content()` |
| BR-6 | Per-subprocess env var injection: extend `AcpClient::spawn` to accept `Vec<(String, String)>` injected via `Command::env()`. Required so `GOOSE_PROVIDER`, `GOOSE_MODEL`, `GOOSE_TEMPERATURE`, and `GOOSE_CONTEXT_LIMIT` can differ per persona without affecting all agents. sprout-acp must check `std::env::var(key)` before injecting — if the parent environment already has the key set, skip injection (operator env vars take precedence, level 1). | `sprout-acp/src/acp.rs` `AcpClient::spawn()` |

---

*End of Persona Pack Specification V7*
