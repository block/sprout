//! Spawn-time loader. Called by `managed_agents::runtime::build_agent_command`
//! when spawning a sprout-agent process — decrypts saved settings and translates
//! them into the env-var pairs sprout-agent expects.
//!
//! ## Plaintext lifetime
//!
//! Plaintext lives only on this function's stack until `apply_to_command`
//! copies it into the child's env block. The plaintext journey:
//!
//! 1. `nip44::decrypt` → `Zeroizing<String>` (wiped at end-of-scope).
//! 2. `serde_json::from_str` → `StoredSettings` (its `Drop` zeroizes
//!    `api_key`; non-secret fields are dropped normally).
//! 3. `stored_to_env_pairs` clones the secret into the returned vec. The
//!    vec is wrapped in a `EnvPairs` newtype whose `Drop` zeroizes every
//!    value buffer, covering panic paths and the spawn-failure path where
//!    the vec is otherwise dropped uneventfully.
//! 4. `apply_to_command` hands each pair to `Command::env(k, v)` by
//!    reference, then immediately zeroizes our owned value buffer before
//!    moving to the next pair. Command's internal env map keeps its own
//!    clone (see "buffers we cannot control" below).
//!
//! ## Threat model — env-as-secret-channel
//!
//! Once we call `Command::env(API_KEY, …)` the secret lives in the child
//! process's env block. On Linux, any other process running as the same user
//! can read `/proc/<pid>/environ`. On macOS, `ps -E` shows env to the same
//! user. On Windows, the same-user case is similar via toolhelp APIs.
//!
//! That is intentional and documented: this feature improves on the prior
//! "export ANTHROPIC_API_KEY in your shell rc" baseline by keeping the key
//! off disk in cleartext and out of casual shell history / screenshots /
//! log shipping. **It does not protect runtime secrets from a same-user
//! attacker** — that would require switching sprout-agent to a fd/file-based
//! secret handoff (out of scope for this PR; tracked separately).
//!
//! ## Buffers we cannot control
//!
//! - **`std::process::Command`'s internal env map.** `Command::env(k, v)`
//!   clones the value into a `BTreeMap<EnvKey, EnvValue>` we don't own.
//!   When the `Command` is dropped (after we call `.spawn()`), that map
//!   drops normally — the bytes are freed without an explicit wipe. There
//!   is no public API to zeroize Command's internal storage.
//! - **The child process's env block.** Once `posix_spawn`/`CreateProcess`
//!   has copied our env vec into the child's address space, those bytes
//!   live for the child's lifetime and are readable via `/proc/<pid>/environ`
//!   on Linux or `ps -E` on macOS (same-user only).
//!
//! What we *do* zeroize: the decrypted `String` (Zeroizing), the parsed
//! `StoredSettings.api_key` (Drop), and every value buffer the `EnvPairs`
//! newtype ever owned (both the hand-off path and Drop, including the
//! panic path). After `apply_to_command` returns, this process holds no
//! plaintext on its own heap — only Command's internal copy remains, and
//! that drops with the Command after spawn.

use tauri::AppHandle;
use zeroize::Zeroize;

use super::storage::{decrypt_settings, read_envelope, settings_path, validate_stored};
use super::storage_profiles::{
    parse_or_migrate_plaintext, write_profiles_envelope, ParseError, ParsedPlaintext,
};
use super::{StoredSettings, OWNED_AGENT_ENV_VARS, PROVIDER_ANTHROPIC, PROVIDER_OPENAI};
use crate::app_state::AppState;

/// Holder for spawn-time env pairs. The pair values contain plaintext API
/// keys; `Drop` zeroizes every value buffer. The drain path also zeroizes
/// each value after it has been handed to the caller (which is expected to
/// borrow it into `Command::env`, not consume it).
///
/// Caveat we cannot avoid: `std::process::Command::env(k, v)` takes
/// `V: AsRef<OsStr>`, then internally clones the bytes into its own env map
/// (a `BTreeMap<EnvKey, EnvValue>`). That internal copy is NOT under our
/// control; on Unix it eventually becomes the child's argv-adjacent env
/// block. We zeroize every buffer we own; Command's internal copy is
/// dropped when the Command is dropped (after spawn), and the child's env
/// block is owned by the kernel/child. This is the "env as secret channel"
/// trade-off documented at the top of this module.
pub struct EnvPairs(Vec<(String, String)>);

impl EnvPairs {
    pub fn new(pairs: Vec<(String, String)>) -> Self {
        Self(pairs)
    }

    /// Hand each (k, v) pair to the supplied closure **by reference**, then
    /// zeroize the value buffer before moving to the next pair. The vec is
    /// drained as we go so that on any panic from `f` (e.g. allocator OOM
    /// inside `Command::env`), the remaining un-handed pairs are still
    /// covered by the `Drop` impl.
    fn drain_into<F: FnMut(&str, &str)>(&mut self, mut f: F) {
        // `drain(..)` yields owned (String, String). We pass them as &str
        // so the caller cannot accidentally move them away from us, and we
        // explicitly zeroize the value after the call returns.
        for (k, mut v) in self.0.drain(..) {
            f(&k, &v);
            v.zeroize();
            // k is not a secret; let it drop normally.
            drop(k);
        }
    }
}

impl Drop for EnvPairs {
    fn drop(&mut self) {
        for (_, v) in self.0.iter_mut() {
            v.zeroize();
        }
        self.0.clear();
    }
}

/// What a spawn site needs to decide how to populate `Command::env`. Spawn
/// policy in `runtime.rs`:
/// - `Ok` → remove OWNED + ACP-level vars, then inject `pairs`.
/// - `None` → no settings file; do nothing (parent env inheritance is fine).
/// - `IdentityMismatch` → settings exist but were saved under a different
///   nsec; **fail closed** like `Error`. Treating it as `None` would let a
///   stale shell `ANTHROPIC_API_KEY` (or any inherited provider var) drive
///   sprout-agent under the wrong identity. The user must explicitly clear
///   or overwrite the settings panel under the current identity.
/// - `Error` → settings file exists and is unreadable; fail closed for owned
///   vars (do not inject; remove inherited values so the agent surfaces a
///   clean "missing required env" error rather than silently using stale shell
///   exports).
pub enum LoadForSpawn {
    Ok(EnvPairs),
    None,
    IdentityMismatch,
    Error(String),
}

/// Load + decrypt the saved settings and translate them into the env-var
/// pairs sprout-agent expects.
///
/// `profile_id`:
/// - `Some(id)` — pick the named profile with exactly this id. Unknown id
///   ⇒ `Error` (no silent fallback to default).
/// - `None` — pick `default_profile_id`. If unset (no default) ⇒ `Error`.
///
/// The caller is `managed_agents::runtime::build_agent_command`, gated by
/// `known_acp_provider(...).id == "sprout-agent"`. We hold the
/// `agent_provider_settings_lock` across read+migrate-write so a settings
/// save in flight cannot land a newer envelope between our read and our
/// best-effort migration write.
pub fn load_for_spawn(app: &AppHandle, state: &AppState, profile_id: Option<&str>) -> LoadForSpawn {
    let path = match settings_path(app) {
        Ok(p) => p,
        Err(e) => return LoadForSpawn::Error(e),
    };

    // Lock order: settings_lock BEFORE state.keys (matches the global lock
    // order documented on AppState::agent_provider_settings_lock).
    let _settings_guard = match state.agent_provider_settings_lock.lock() {
        Ok(g) => g,
        Err(e) => return LoadForSpawn::Error(format!("settings lock poisoned: {e}")),
    };

    let envelope = match read_envelope(&path) {
        Ok(Some(e)) => e,
        Ok(None) => return LoadForSpawn::None,
        Err(e) => return LoadForSpawn::Error(e),
    };
    let keys = match state.keys.lock() {
        Ok(k) => k,
        Err(e) => return LoadForSpawn::Error(format!("keys lock poisoned: {e}")),
    };
    let current_pubkey = keys.public_key().to_hex();
    if envelope.pubkey != current_pubkey {
        return LoadForSpawn::IdentityMismatch;
    }
    let plain = match decrypt_settings(&keys, &envelope.ciphertext) {
        Ok(p) => p,
        Err(e) => return LoadForSpawn::Error(e),
    };

    let parsed = match parse_or_migrate_plaintext(&plain, &envelope.pubkey) {
        Ok(p) => p,
        // Cross-check failure (wrapper/legacy owner_pubkey vs envelope)
        // surfaces as IdentityMismatch — same threat model as today's
        // envelope.pubkey check (a swapped-in envelope from another id).
        Err(ParseError::IdentityMismatch) => return LoadForSpawn::IdentityMismatch,
        Err(ParseError::Other(msg)) => {
            return LoadForSpawn::Error(format!("parse profiles plaintext: {msg}"));
        }
    };

    // Best-effort migration write (spawn path is non-fatal per plan §4):
    // if the write fails (read-only fs, disk full), still proceed with the
    // in-memory wrapper so an agent boot isn't blocked. Read commands DO
    // surface this error — see commands.rs.
    let wrapper = match parsed {
        ParsedPlaintext::Current(w) => w,
        ParsedPlaintext::Migrated(w) => {
            if let Err(e) = write_profiles_envelope(&path, &keys, &w) {
                eprintln!(
                    "sprout-desktop: agent-provider settings migration write failed (spawn \
                     proceeds with in-memory profile): {e}"
                );
            }
            w
        }
    };
    drop(keys);

    // Wrapper-level owner cross-check redundant with parse_or_migrate_plaintext,
    // but cheap and defensive.
    if wrapper.owner_pubkey != envelope.pubkey || wrapper.owner_pubkey != current_pubkey {
        return LoadForSpawn::IdentityMismatch;
    }

    let chosen = match resolve_target_profile(&wrapper, profile_id, &current_pubkey) {
        Ok(p) => p,
        Err(load) => return load,
    };

    // Fail closed if the decrypted plaintext would not pass save-time
    // validation. Catches envelopes written by older builds (before the
    // current control-char / non-loopback-HTTP / size-cap checks existed).
    if let Err(e) = validate_stored(&chosen.settings) {
        return LoadForSpawn::Error(format!("stored settings failed validation: {e}"));
    }
    LoadForSpawn::Ok(EnvPairs::new(stored_to_env_pairs(&chosen.settings)))
}

/// Pure wrapper→profile selector. Split out of `load_for_spawn` so it's
/// directly unit-testable without faking `AppHandle`/`AppState`.
///
/// Resolution policy:
/// - `profile_id = Some(id)` → must exist; unknown id ⇒ `Error` (no silent
///   fallback to default).
/// - `profile_id = None` → use `wrapper.default_profile_id`; missing default
///   or default points at unknown id ⇒ `Error`.
///
/// Per-profile owner cross-check runs on the chosen profile (plan §6 step 7):
/// catches a tampered wrapper that shuffled an old-identity `NamedProfile`
/// into a current-identity wrapper. NIP-44 authenticates the outer
/// ciphertext, not the inner per-profile bytes, so the per-profile owner
/// hash needs to be re-verified against the wrapper and the current
/// identity. Mismatch ⇒ `IdentityMismatch`.
pub(super) fn resolve_target_profile<'w>(
    wrapper: &'w super::ProfilesPlaintext,
    profile_id: Option<&str>,
    current_pubkey: &str,
) -> Result<&'w super::NamedProfile, LoadForSpawn> {
    let chosen = match profile_id {
        Some(id) => wrapper
            .profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| {
                LoadForSpawn::Error(format!(
                    "agent references profile id {id} which is not in saved settings"
                ))
            })?,
        None => match wrapper.default_profile_id.as_deref() {
            Some(default_id) => wrapper
                .profiles
                .iter()
                .find(|p| p.id == default_id)
                .ok_or_else(|| {
                    LoadForSpawn::Error(
                        "default_profile_id references a profile that does not exist".into(),
                    )
                })?,
            None => {
                return Err(LoadForSpawn::Error(
                    "no default provider profile is set; pick one in Sprout Settings → \
                     Agent Provider, or assign a profile to this agent"
                        .into(),
                ));
            }
        },
    };

    if chosen.settings.schema_version >= 2
        && !chosen.settings.owner_pubkey.is_empty()
        && (chosen.settings.owner_pubkey != wrapper.owner_pubkey
            || chosen.settings.owner_pubkey != current_pubkey)
    {
        return Err(LoadForSpawn::IdentityMismatch);
    }

    Ok(chosen)
}

pub(super) fn stored_to_env_pairs(s: &StoredSettings) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = Vec::with_capacity(16);
    pairs.push(("SPROUT_AGENT_PROVIDER".into(), s.provider.clone()));

    match s.provider.as_str() {
        PROVIDER_ANTHROPIC => {
            pairs.push(("ANTHROPIC_API_KEY".into(), s.api_key.clone()));
            pairs.push(("ANTHROPIC_MODEL".into(), s.model.clone()));
            pairs.push(("ANTHROPIC_BASE_URL".into(), s.base_url.clone()));
            if let Some(v) = &s.anthropic_api_version {
                pairs.push(("ANTHROPIC_API_VERSION".into(), v.clone()));
            }
        }
        PROVIDER_OPENAI => {
            pairs.push(("OPENAI_COMPAT_API_KEY".into(), s.api_key.clone()));
            pairs.push(("OPENAI_COMPAT_MODEL".into(), s.model.clone()));
            pairs.push(("OPENAI_COMPAT_BASE_URL".into(), s.base_url.clone()));
        }
        // Validation in save_agent_provider_settings prevents other values.
        _ => {}
    }

    if let Some(p) = &s.system_prompt {
        // Don't emit an empty string — sprout-agent treats unset vs empty differently.
        if !p.is_empty() {
            pairs.push(("SPROUT_AGENT_SYSTEM_PROMPT".into(), p.clone()));
        }
    }
    if let Some(n) = s.max_rounds {
        pairs.push(("SPROUT_AGENT_MAX_ROUNDS".into(), n.to_string()));
    }
    if let Some(n) = s.max_output_tokens {
        pairs.push(("SPROUT_AGENT_MAX_OUTPUT_TOKENS".into(), n.to_string()));
    }
    if let Some(n) = s.llm_timeout_secs {
        pairs.push(("SPROUT_AGENT_LLM_TIMEOUT_SECS".into(), n.to_string()));
    }
    if let Some(n) = s.tool_timeout_secs {
        pairs.push(("SPROUT_AGENT_TOOL_TIMEOUT_SECS".into(), n.to_string()));
    }
    if let Some(n) = s.max_history_bytes {
        pairs.push(("SPROUT_AGENT_MAX_HISTORY_BYTES".into(), n.to_string()));
    }
    pairs
}

/// Apply the load-for-spawn result to a partially-built `Command`. Pure
/// function — no I/O — so it's directly unit-testable.
///
/// Always clears ACP-level overrides (`SPROUT_ACP_MODEL`, `SPROUT_ACP_SYSTEM_PROMPT`)
/// because for sprout-agent those are owned by the Agent Provider settings
/// panel rather than the per-agent record. Then applies the load result:
///
/// - `Ok(pairs)` → strip owned vars + system-prompt-file, inject pairs.
/// - `None` → leave env alone (shell-env fallback).
/// - `IdentityMismatch` → leave env alone, log a warning so the user can
///   correlate with the panel's banner.
/// - `Error(e)` → fail closed: strip owned vars so we don't silently use
///   stale shell exports. The agent's own "missing required env" error
///   surfaces in its log.
pub fn apply_to_command(command: &mut std::process::Command, load: LoadForSpawn, agent_name: &str) {
    // Unconditional: clear ACP-level overrides for sprout-agent.
    command.env_remove("SPROUT_ACP_MODEL");
    command.env_remove("SPROUT_ACP_SYSTEM_PROMPT");

    match load {
        LoadForSpawn::Ok(mut pairs) => {
            for k in OWNED_AGENT_ENV_VARS {
                command.env_remove(k);
            }
            command.env_remove("SPROUT_AGENT_SYSTEM_PROMPT_FILE");
            // `drain_into` empties the underlying vec, zeroizing each value
            // immediately after `Command::env` has copied it into Command's
            // internal env map. EnvPairs::Drop is then a no-op on an empty
            // vec. If `command.env(...)` panics (it shouldn't), Drop covers
            // whatever's left.
            pairs.drain_into(|k, v| {
                command.env(k, v);
            });
        }
        LoadForSpawn::None => {
            // No settings — shell-env fallback per rule 2.
        }
        LoadForSpawn::IdentityMismatch => {
            // Fail closed: a settings file exists but it's not for the
            // currently logged-in identity. Inheriting shell env here would
            // silently run the agent under the wrong nsec with whatever
            // ANTHROPIC_API_KEY / OPENAI_COMPAT_API_KEY happens to be in the
            // parent shell. Strip owned vars so the agent surfaces a clean
            // "missing required env" error and the user has to resolve the
            // mismatch explicitly (clear/overwrite from the panel).
            eprintln!(
                "sprout-desktop: agent provider settings exist for a different identity — \
                 failing closed for {agent_name}",
            );
            for k in OWNED_AGENT_ENV_VARS {
                command.env_remove(k);
            }
            command.env_remove("SPROUT_AGENT_SYSTEM_PROMPT_FILE");
        }
        LoadForSpawn::Error(e) => {
            eprintln!(
                "sprout-desktop: agent provider settings unreadable for {agent_name} — \
                 failing closed: {e}",
            );
            for k in OWNED_AGENT_ENV_VARS {
                command.env_remove(k);
            }
            command.env_remove("SPROUT_AGENT_SYSTEM_PROMPT_FILE");
        }
    }
}
