# Vision: sprout-agent + sprout-dev-mcp

## The Problem

General-purpose coding agents are tens of thousands of lines of code. They have plugin systems, recipe systems, multiple transport modes, session persistence, auto-compaction, configuration management, and extension architectures. They work. They are also impossible to hold in your head.

When something goes wrong, you cannot reason about it. When you want to change behavior, you are fighting abstractions three layers deep. When you want to run ten agents in parallel, you are paying for that entire surface area per instance.

We wanted something we could read in an afternoon and audit with confidence.

## What We Built

Two binaries. Two protocols. Zero coupling.

**sprout-agent** (~2,100 LOC) is an ACP agent. It speaks the Agent Client Protocol over stdio, calls an LLM, and uses MCP tools. One session. One prompt in flight. When context fills up, it summarizes and resets internally. The client never knows. It works with Zed, JetBrains, sprout-acp, or anything else that speaks ACP.

**sprout-dev-mcp** (~1,100 LOC) is an MCP server. It gives any agent a shell and a file editor. Ephemeral processes with process-group kill on every exit path. Bounded output. Workspace-sandboxed file edits. It works with any agent or client that speaks MCP.

Together they are ~3,200 lines of Rust that replace a general-purpose agent for headless autonomous coding work.

## Why We Built Our Own

**Auditability.** Every line earns its keep. A senior engineer reads the entire codebase in one sitting. There are no abstractions that exist for future flexibility. When the agent does something unexpected, you trace the exact path in minutes.

**Correctness at the boundary.** ACP compliance is not a checkbox. We report a concrete protocol version. We emit every required notification. We handle cancellation on every path. We kill process trees on timeout. Key safety properties have regression tests that lock them down.

**Composability through standards.** The agent does not know what MCP server it talks to. The MCP server does not know what agent is calling it. They compose through protocols, not imports. You can run ten agents behind sprout with different MCP configurations. You can swap the LLM provider with one environment variable. You can point Zed at sprout-agent and get the same tool-calling behavior in your editor.

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

Two pipes. Two protocols. The agent's output is its tool calls. Text is internal reasoning. The tools do the work.

## Design Principles

- **Minimal.** If you can delete it, delete it. We deleted the TODO tool, context injection, ast-grep, streaming, persistence, multi-session support, and a provider trait. Each deletion made the system better.

- **Hardened.** Zero unsafe. Zero panics. Bounded everything. Process-group kill on every exit path. File edits are workspace-sandboxed. The shell runs at the operator's trust level, like bash itself. History validity is maintained on every cancellation path. The system degrades gracefully, with bounded failure modes.

- **Protocol-native.** ACP is the only interface to the agent. MCP is the only interface to the tools. No runtime coupling. No shared state. No custom wire formats.

- **Honest.** The agent is a loop: prompt the LLM, execute tool calls, repeat. When context fills, it hands off to itself. When it cannot proceed, it stops. No heroics, no heuristics, no hidden complexity.

## What This Enables

- Ten agents in parallel behind sprout, each with their own MCP configuration
- Any ACP client gets a coding agent for free
- Any MCP server gets a capable caller for free
- A codebase small enough to fork, modify, and understand in a day
- Benchmark evaluation with a thin Python wrapper

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
