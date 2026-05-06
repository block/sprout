# Vision: sprout-agent + sprout-dev-mcp

## The Problem

A coding agent should be small enough to hold in your head. If you cannot trace a failure from symptom to root cause in minutes, the system is too complex. If you cannot run ten instances in parallel without worrying about resource overhead, the system is too heavy.

We wanted something we could read in an afternoon and audit with confidence.

## What We Built

Two binaries, two protocols, no coupling between them.

**sprout-agent** (~2,100 LOC) is an ACP agent. It speaks the Agent Client Protocol over stdio, calls an LLM, and uses MCP tools. One session, one prompt in flight. When context fills up, it summarizes its own history and continues. The client sees a single coherent session. It works with Zed, JetBrains, sprout-acp, or anything else that speaks ACP.

**sprout-dev-mcp** (~1,100 LOC) is an MCP server. It gives any agent a shell and a file editor. Ephemeral processes with process-group kill on every exit path. Bounded output. Workspace-sandboxed file edits. It works with any agent or client that speaks MCP.

Together they are ~3,200 lines of Rust purpose-built for headless autonomous coding work.

## Why We Built Our Own

**Auditability.** A senior engineer can read both binaries in a sitting. There are no abstractions reserved for future flexibility. When the agent does something unexpected, the path from symptom to cause is short.

**Correctness at the boundary.** ACP compliance is not a checkbox. We report a concrete protocol version. We emit every required notification. We handle cancellation on every path. We kill process trees on timeout. Key safety properties have regression tests that lock them down.

**Composability through standards.** The agent does not know what MCP server it talks to. The MCP server does not know what agent is calling it. They compose through protocols, not imports. Run ten agents behind sprout with different MCP configurations. Swap the LLM provider with one environment variable. Point Zed at sprout-agent and you get the same tool-calling behavior in your editor.

## The Architecture

```
Any ACP client (Zed, JetBrains, sprout-acp, custom)
        |
        | stdio ACP (JSON-RPC 2.0)
        v
  sprout-agent
        |
        | stdio MCP (JSON-RPC 2.0)
        v
  sprout-dev-mcp (or any MCP server)
        |
        v
  shell, str_replace; rg on PATH
```

Two pipes. Two protocols. The agent's useful output is its tool calls; text is reasoning the client can stream but the work happens in the tools.

## Design Principles

- **Minimal.** If you can delete it, delete it. We deleted the TODO tool, context injection, ast-grep, streaming, persistence, multi-session support, and a provider trait. The system got better with each one gone.

- **Hardened.** Zero unsafe. Zero panics. Bounded process lifetime, bounded output sizes, bounded history. Process-group kill on every exit path. File edits are workspace-sandboxed. The shell runs at the operator's trust level, like bash itself. History validity is maintained on every cancellation path. The system degrades gracefully, with bounded failure modes.

- **Protocol-native.** ACP is the only interface to the agent. MCP is the only interface to the tools. No runtime coupling. No shared state. No custom wire formats.

- **Honest.** The agent is a loop: prompt the LLM, execute tool calls, repeat. When context fills, it hands off to itself. When it cannot proceed, it stops. The handoff is a heuristic; everything else is mechanical.

## What This Enables

- Ten agents in parallel behind sprout, each with their own MCP configuration
- Any ACP client gets a coding agent without a custom adapter
- Any MCP server gets a capable caller without a custom adapter
- A codebase small enough to fork, modify, and understand in a day

## The Numbers

| | sprout-agent | sprout-dev-mcp |
|---|---|---|
| Production LOC | ~2,100 | ~1,100 |
| Source files | 7 | 5 |
| Direct dependencies | 7 | 8 |
| Tests | 25 | 14 |
| Unsafe blocks | 0 | 0 |
| Panic paths (expect/unwrap) | 0 | 0 |
| Works with any ACP client | yes | n/a |
| Works with any MCP client | n/a | yes |
