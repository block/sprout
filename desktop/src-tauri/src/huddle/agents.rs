//! Agent enrollment for huddles.
//!
//! Mental model:
//!   add_agent_to_huddle → kind:9000 to ephemeral channel
//!                       → kind:9000 to parent channel (best-effort)
//!
//! ACP spawning is NOT needed here: the running agent process auto-subscribes
//! when it receives the kind:9000 membership notification. Huddle-specific
//! env vars (interrupt mode, custom system prompt) are a post-MVP enhancement.

use serde::Serialize;
use uuid::Uuid;

use crate::{app_state::AppState, events, relay::submit_event};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Voice-mode guidelines posted as a kind:9 message (with [System] prefix)
/// to the ephemeral channel at huddle start. Instructs agents on voice-mode
/// etiquette: TTS constraints, brevity rules, self-selection.
/// Build voice-mode guidelines with the parent channel ID so agents know
/// where "the main channel" is.
pub fn voice_mode_guidelines(parent_channel_id: &str) -> String {
    format!(
        "\
You are in a live voice huddle. Your text is read aloud via TTS.
This huddle is attached to channel {parent_channel_id} — that's the main channel.
You will be interrupted by new messages whenever a human speaks — this is normal.

Rules:
- Only respond if the message is relevant to you or directed at you.
  If it's not for you, respond with just \".\" or stay silent.
- Keep responses under 2 sentences. This is a conversation, not an essay.
- Spell out numbers: \"eleven thirty\" not \"11:30\".
- No markdown, code blocks, or bullet lists — they sound terrible as speech.
- To share code or data, say \"I'll post that in the main channel\" and use it.
- You have access to Sprout tools — you can join channels, search messages,
  and take actions. Use them proactively when asked."
    )
}

// ── Agent enrollment ──────────────────────────────────────────────────────────

/// Result of adding an agent to a huddle.
///
/// `ephemeral_added` is always true on success (the function returns Err if
/// the ephemeral add fails). `parent_added` reflects whether the parent-channel
/// add succeeded; `parent_error` carries the error string when it didn't.
#[derive(Debug, Serialize)]
pub struct AgentAddResult {
    /// Whether the agent was added to the ephemeral channel (required).
    pub ephemeral_added: bool,
    /// Whether the agent was also added to the parent channel (best-effort).
    pub parent_added: bool,
    /// Error from the parent-channel add, if it failed.
    pub parent_error: Option<String>,
}

/// Add an agent to both the ephemeral and parent huddle channels.
///
/// Returns `Err` only if the ephemeral-channel add fails (policy rejection or
/// network error). The parent-channel add is best-effort: failure is captured
/// in `AgentAddResult::parent_error` rather than propagated.
///
/// The running ACP process for this agent will auto-subscribe to the new
/// channel when it receives the kind:9000 membership notification.
pub async fn add_agent_to_huddle(
    ephemeral_channel_id: Uuid,
    parent_channel_id: Uuid,
    agent_pubkey: &str,
    state: &AppState,
) -> Result<AgentAddResult, String> {
    // 1. Add agent to ephemeral channel (required — fail hard on rejection).
    let add_eph = events::build_add_member(ephemeral_channel_id, agent_pubkey, None)?;
    submit_event(add_eph, state).await?;

    // 2. Add agent to parent channel — so agent has full context.
    //    Best-effort: capture the error but don't propagate it.
    let (parent_added, parent_error) = {
        let add_parent = events::build_add_member(parent_channel_id, agent_pubkey, None)?;
        match submit_event(add_parent, state).await {
            Ok(_) => (true, None),
            Err(e) => {
                eprintln!(
                    "sprout-desktop: add agent to parent channel failed (may already be member): {e}"
                );
                (false, Some(e))
            }
        }
    };

    Ok(AgentAddResult {
        ephemeral_added: true,
        parent_added,
        parent_error,
    })
}
