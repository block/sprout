# Keep-Up Agents — Feature Spec (v2)

**Branch:** `feature/keep-up-agents`
**Author:** Alice (agent)
**Date:** 2026-03-17
**Status:** Revised after crossfire review (GPT-5 + Codex)

---

## Problem

When the Sprout desktop app starts, managed agents that were previously running
are in a `stopped` state. The user must manually navigate to the Agents pane
and click "Spawn" on each one. For users who always want their agents online,
this is tedious friction.

## Solution

Add a **"Keep Up"** toggle per managed agent. When enabled, the desktop app
automatically attempts to spawn the agent **once** on app startup if the agent
is not already running. If the spawn fails, it records the failure persistently
and shows a status indicator — it does **not** retry.

## Design Principles

- **One attempt, no retry.** Auto-spawn fires once per app launch per agent.
  No exponential backoff, no polling loop. The user can always manually spawn.
- **Opt-in for existing, opt-out for new.** New agents default to
  `keep_up = true`. Existing agents (pre-migration) default to `false` — we
  do not auto-launch previously configured commands without explicit consent.
- **Desktop-only.** Purely a desktop app feature. No relay changes, no new
  API endpoints, no MCP tool changes.
- **Minimal surface.** One new boolean field in the record, one new Tauri
  command for the auto-spawn orchestration, one checkbox in the UI.

---

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│  Tauri app startup (setup hook in lib.rs)                     │
│                                                              │
│  auto_spawn_keep_up_agents(app, state)                       │
│    ├─ loads records, acquires locks                           │
│    ├─ filters agents where keep_up=true && not running       │
│    ├─ calls start_managed_agent_process() once per agent     │
│    ├─ on failure: sets record.last_error, saves              │
│    └─ runs BEFORE frontend mounts — fire-and-forget          │
│                                                              │
│  ManagedAgentCard (frontend)                                  │
│    ├─ [✓] Keep Up  (checkbox, toggles keep_up field)          │
│    └─ existing lastError shows auto-spawn failures too       │
└──────────────────────────────────────────────────────────────┘
```

**Key architectural decision:** Auto-spawn runs in the **Tauri backend setup
hook**, not in a React component. This guarantees:
1. It runs exactly once per app launch (no mount/unmount issues).
2. It runs regardless of which view the user navigates to first.
3. No React ref/state lifecycle concerns.
4. Errors are persisted in `last_error` on the record — the existing UI
   already renders this field.

---

## Changes by Layer

### 1. Tauri Backend — `ManagedAgentRecord` (types.rs)

Add one field:

```rust
#[serde(default)] // false for existing records — no surprise auto-launch
pub keep_up: bool,
```

Default is `false` via `Default` for `bool`. Existing `managed-agents.json`
files without the field get `keep_up = false` — existing agents are NOT
auto-opted-in.

New agents created via `CreateManagedAgentRequest` get `keep_up: true` from
the create handler (explicit in code).

Add to `ManagedAgentSummary`:

```rust
pub keep_up: bool,
```

Update `build_managed_agent_summary` to copy the field through.

### 2. Tauri Backend — Auto-Spawn on Startup (new function in runtime.rs)

```rust
pub fn auto_spawn_keep_up_agents(
    app: &AppHandle,
    store_lock: &Mutex<()>,
    processes: &Mutex<HashMap<String, ManagedAgentProcess>>,
) -> Result<(), String> {
    let _guard = store_lock.lock().map_err(|e| e.to_string())?;
    let mut records = load_managed_agents(app)?;
    let mut runtimes = processes.lock().map_err(|e| e.to_string())?;

    sync_managed_agent_processes(&mut records, &mut runtimes);

    let mut changed = false;
    let keep_up_pubkeys: Vec<String> = records
        .iter()
        .filter(|r| r.keep_up && !runtimes.contains_key(&r.pubkey))
        .map(|r| r.pubkey.clone())
        .collect();

    for pubkey in keep_up_pubkeys {
        let record = find_managed_agent_mut(&mut records, &pubkey)?;
        if let Err(error) = start_managed_agent_process(app, record, &mut runtimes) {
            record.last_error = Some(format!("Auto-spawn failed: {error}"));
            record.updated_at = now_iso();
            changed = true;
        } else {
            changed = true;
        }
    }

    if changed {
        save_managed_agents(app, &records)?;
    }

    Ok(())
}
```

Called from the Tauri `setup` hook in `lib.rs`:

```rust
.setup(|app| {
    // ... existing setup ...
    let state = app.state::<AppState>();
    if let Err(error) = auto_spawn_keep_up_agents(
        app.handle(),
        &state.managed_agents_store_lock,
        &state.managed_agent_processes,
    ) {
        eprintln!("sprout-desktop: keep-up auto-spawn failed: {error}");
    }
    Ok(())
})
```

### 3. Tauri Backend — Toggle Command (commands/agents.rs)

```rust
#[tauri::command]
pub fn set_managed_agent_keep_up(
    pubkey: String,
    keep_up: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ManagedAgentSummary, String>
```

Loads records, finds the agent, sets `record.keep_up = keep_up`, saves, and
returns the updated summary. **No spawn side-effects** — toggling on mid-session
does not trigger a spawn. It only affects the next app launch.

Register in `lib.rs` alongside the other agent commands.

### 4. Frontend Types (shared/api/types.ts)

Add to `ManagedAgent`:

```typescript
keepUp: boolean;
```

### 5. RawManagedAgent (shared/api/tauri.ts)

Add to `RawManagedAgent`:

```typescript
keep_up: boolean;
```

Update `fromRawManagedAgent` to map `keep_up` → `keepUp`.

### 6. Tauri API Bridge (shared/api/tauri.ts)

```typescript
export async function setManagedAgentKeepUp(
  pubkey: string,
  keepUp: boolean,
): Promise<ManagedAgent> {
  const response = await invokeTauri<RawManagedAgent>(
    "set_managed_agent_keep_up",
    { pubkey, keepUp },
  );
  return fromRawManagedAgent(response);
}
```

### 7. E2E Bridge (testing/e2eBridge.ts)

Add `keep_up: boolean` to the mock `RawManagedAgent` type and
`MockManagedAgent`. Add handler for `set_managed_agent_keep_up` command.
Default `keep_up` to `true` in `handleCreateManagedAgent` and `false` in
`resetMockManagedAgents`.

### 8. Agent Hooks (features/agents/hooks.ts)

Add `useSetManagedAgentKeepUpMutation`:

```typescript
export function useSetManagedAgentKeepUpMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ pubkey, keepUp }: { pubkey: string; keepUp: boolean }) =>
      setManagedAgentKeepUp(pubkey, keepUp),
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: managedAgentsQueryKey });
    },
  });
}
```

**No `useKeepUpAutoSpawn` hook.** Auto-spawn is handled entirely in the Tauri
backend setup hook. The frontend is purely a toggle + display.

### 9. UI — ManagedAgentCard.tsx

Add a checkbox in the agent card header area:

```tsx
<label className="flex items-center gap-2 text-xs text-muted-foreground">
  <input
    type="checkbox"
    checked={agent.keepUp}
    onChange={(e) => onToggleKeepUp(agent.pubkey, e.target.checked)}
    className="rounded border-border"
  />
  Keep up
</label>
```

Auto-spawn errors surface through the existing `lastError` field — no new
error UI needed. The existing error block at the bottom of the card already
renders `agent.lastError`.

### 10. Wiring — AgentsView.tsx + ManagedAgentsSection.tsx

- Call `useSetManagedAgentKeepUpMutation()` for the toggle handler.
- Pass `onToggleKeepUp` down through `ManagedAgentsSection` →
  `ManagedAgentCard`.
- No auto-spawn hook, no ephemeral error state.

### 11. CreateAgentDialog

The `CreateManagedAgentRequest` already has a `spawn_after_create` field.
Add `keep_up` (defaults to `true` in the backend create handler). No UI
change needed in the create dialog — the default is correct.

---

## What This Does NOT Do

- **No persistent retry / watchdog.** One attempt per app launch. Period.
- **No relay-side changes.** No new event kinds, no new REST endpoints.
- **No presence-based spawning.** We check local process state, not relay
  presence. The desktop app owns the process.
- **No reconnect handling.** This feature is about app startup only. Relay
  reconnect is a separate concern.
- **No mid-session auto-spawn.** Toggling "Keep Up" on does not immediately
  spawn. It takes effect on the next app launch.

---

## Files Modified

| File | Change |
|------|--------|
| `desktop/src-tauri/src/managed_agents/types.rs` | Add `keep_up` to `ManagedAgentRecord`, `ManagedAgentSummary`, `CreateManagedAgentRequest` |
| `desktop/src-tauri/src/managed_agents/runtime.rs` | Copy `keep_up` in summary builder; add `auto_spawn_keep_up_agents` |
| `desktop/src-tauri/src/commands/agents.rs` | Add `set_managed_agent_keep_up` command; set `keep_up = true` in create handler |
| `desktop/src-tauri/src/lib.rs` | Register new command; call auto-spawn in setup hook |
| `desktop/src/shared/api/types.ts` | Add `keepUp: boolean` to `ManagedAgent` |
| `desktop/src/shared/api/tauri.ts` | Add `keep_up` to `RawManagedAgent`; add `setManagedAgentKeepUp`; update mapper |
| `desktop/src/testing/e2eBridge.ts` | Add `keep_up` to mock types; add command handler |
| `desktop/src/features/agents/hooks.ts` | Add `useSetManagedAgentKeepUpMutation` |
| `desktop/src/features/agents/ui/AgentsView.tsx` | Wire toggle handler |
| `desktop/src/features/agents/ui/ManagedAgentsSection.tsx` | Pass through `onToggleKeepUp` prop |
| `desktop/src/features/agents/ui/ManagedAgentCard.tsx` | Add "Keep up" checkbox |

---

## Edge Cases

| Case | Behavior |
|------|----------|
| Agent already running on app start | `runtimes.contains_key()` → skip |
| Spawn fails (missing binary, port conflict) | `last_error` set with "Auto-spawn failed: ..." prefix, persisted, shown in existing UI |
| User toggles Keep Up on mid-session | Persisted. Takes effect on next app launch. No immediate spawn. |
| User toggles Keep Up off | Persisted. Next launch skips this agent. |
| Multiple agents with Keep Up | All attempted sequentially in the setup hook (same lock scope) |
| Existing agents.json without `keep_up` field | `serde(default)` → `false` — no surprise auto-launch |
| New agents | `keep_up = true` set in create handler |
| User manually stops agent after auto-spawn | Stays stopped. No re-spawn until next app launch. |
| Auto-spawn error then manual spawn succeeds | `last_error` cleared by `start_managed_agent_process` (existing behavior) |

---

## Review Feedback Addressed

| Reviewer | Issue | Resolution |
|----------|-------|------------|
| GPT-5 | Startup vs. Agents-pane contradiction | **Fixed.** Auto-spawn moved to Tauri setup hook — runs on app startup, not view mount. |
| GPT-5 | `useRef` resets on remount | **Fixed.** No React hook for auto-spawn. Backend-only. |
| GPT-5 | Failure state underspecified | **Fixed.** Errors persisted in `last_error` field, rendered by existing UI. |
| GPT-5 | Reconnect mentioned but undefined | **Fixed.** Removed from problem statement. Startup only. |
| GPT-5 | Default true too aggressive for existing | **Fixed.** Existing agents default to `false`. New agents default to `true`. |
| Codex | Toggle on triggers immediate spawn | **Fixed.** Toggle is preference-only. No spawn side-effect. |
| Codex | e2eBridge not updated | **Fixed.** Added to files modified list with specific changes. |
| Codex | Reuse `last_error` instead of ephemeral state | **Fixed.** Auto-spawn errors go through `last_error` with "Auto-spawn failed:" prefix. |
| Codex | Don't piggyback on start mutation for boot | **Fixed.** Auto-spawn calls `start_managed_agent_process` directly in backend. |

---

## Testing

- **Unit:** Verify `serde(default)` deserializes old records with
  `keep_up = false`. Verify new records get `keep_up = true`.
- **Integration:** Create agent with `keep_up: true`, stop it, restart app —
  verify it auto-spawns. Create agent with `keep_up: false` — verify it stays
  stopped.
- **E2E (manual):** Launch app with a stopped keep-up agent. Verify it
  auto-spawns. Kill the binary it depends on, relaunch — verify `lastError`
  shows "Auto-spawn failed: ..." and no retry loop.
- **E2E bridge:** Verify mock handles `set_managed_agent_keep_up` command
  and returns updated agent with toggled `keep_up`.
