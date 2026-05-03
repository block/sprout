//! NIP-AA — Agent Authentication via NIP-OA Credentials
//!
//! When a pubkey is not a direct relay member, check if the AUTH event
//! carries a valid NIP-OA `auth` tag whose owner is an active member.

use nostr::PublicKey;
use tracing::debug;

use crate::state::AppState;

/// Result of NIP-AA verification.
pub struct NipAaResult {
    /// The owner pubkey that granted virtual membership.
    pub owner_pubkey: PublicKey,
}

/// Extract and verify a NIP-OA auth tag from event tags for NIP-AA authentication.
///
/// Implements NIP-AA Steps 3-5:
/// - Step 3: Extract exactly one `auth` tag (zero or >1 → None)
/// - Step 4: Verify the auth tag cryptographically (reuses sprout-sdk nip_oa)
/// - Step 4b: Evaluate created_at conditions against the event's created_at
/// - Step 5: Check that the owner is an active relay member
///
/// Returns `Ok(Some(NipAaResult))` if NIP-AA grants access.
/// Returns `Ok(None)` if no auth tag is present (not an agent).
/// Returns `Err(reason)` if auth tag is present but invalid.
pub async fn verify_nip_aa(
    state: &AppState,
    agent_pubkey: &PublicKey,
    tags: &[nostr::Tag],
    event_created_at: u64,
) -> Result<Option<NipAaResult>, String> {
    // Step 3: Extract exactly one auth tag
    let auth_tags: Vec<&nostr::Tag> = tags
        .iter()
        .filter(|t| {
            let s = t.as_slice();
            !s.is_empty() && s[0] == "auth"
        })
        .collect();

    if auth_tags.is_empty() {
        return Ok(None); // No auth tag — not an agent, not a NIP-AA attempt
    }

    if auth_tags.len() > 1 {
        return Err("restricted: multiple auth tags present".to_string());
    }

    let auth_tag = auth_tags[0];
    let tag_json = serde_json::to_string(&auth_tag.as_slice())
        .map_err(|e| format!("restricted: failed to serialize auth tag: {e}"))?;

    // Step 4: Verify the auth tag cryptographically
    let owner_pubkey = match sprout_sdk::nip_oa::verify_auth_tag(&tag_json, agent_pubkey) {
        Ok(pk) => pk,
        Err(e) => {
            return Err(format!("restricted: invalid auth tag: {e}"));
        }
    };

    // Step 4b: Evaluate created_at conditions
    // Parse conditions from the auth tag (element [2])
    let tag_slice = auth_tag.as_slice();
    if tag_slice.len() >= 3 {
        let conditions = &tag_slice[2];
        if !conditions.is_empty() {
            if let Err(reason) = evaluate_created_at_conditions(conditions, event_created_at) {
                return Err(format!("restricted: {reason}"));
            }
        }
    }

    // Step 5: Check owner is an active relay member
    let owner_hex = owner_pubkey.to_hex();
    let is_member = state
        .db
        .is_relay_member(&owner_hex)
        .await
        .map_err(|e| format!("restricted: owner membership check failed: {e}"))?;

    if !is_member {
        return Err("restricted: owner is not a relay member".to_string());
    }

    debug!(
        agent = %agent_pubkey.to_hex(),
        owner = %owner_hex,
        "NIP-AA: virtual membership granted"
    );

    Ok(Some(NipAaResult { owner_pubkey }))
}

/// Evaluate `created_at<t` and `created_at>t` conditions against an event's created_at.
/// Returns Ok(()) if all conditions pass, Err(reason) if any fail.
/// `kind=` conditions are intentionally skipped per NIP-AA spec.
fn evaluate_created_at_conditions(conditions: &str, event_created_at: u64) -> Result<(), String> {
    for clause in conditions.split('&') {
        if clause.is_empty() {
            continue;
        }
        if let Some(val_str) = clause.strip_prefix("created_at<") {
            let threshold: u64 = val_str
                .parse()
                .map_err(|_| format!("malformed created_at< condition: {clause}"))?;
            if event_created_at >= threshold {
                return Err(format!(
                    "created_at condition not satisfied: {event_created_at} >= {threshold}"
                ));
            }
        } else if let Some(val_str) = clause.strip_prefix("created_at>") {
            let threshold: u64 = val_str
                .parse()
                .map_err(|_| format!("malformed created_at> condition: {clause}"))?;
            if event_created_at <= threshold {
                return Err(format!(
                    "created_at condition not satisfied: {event_created_at} <= {threshold}"
                ));
            }
        }
        // kind= clauses are intentionally skipped at admission per NIP-AA §Kind Conditions
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_conditions_passes() {
        assert!(evaluate_created_at_conditions("", 1000).is_ok());
    }

    #[test]
    fn created_at_lt_passes() {
        assert!(evaluate_created_at_conditions("created_at<2000", 1000).is_ok());
    }

    #[test]
    fn created_at_lt_fails_when_equal() {
        assert!(evaluate_created_at_conditions("created_at<1000", 1000).is_err());
    }

    #[test]
    fn created_at_lt_fails_when_greater() {
        assert!(evaluate_created_at_conditions("created_at<500", 1000).is_err());
    }

    #[test]
    fn created_at_gt_passes() {
        assert!(evaluate_created_at_conditions("created_at>500", 1000).is_ok());
    }

    #[test]
    fn created_at_gt_fails_when_equal() {
        assert!(evaluate_created_at_conditions("created_at>1000", 1000).is_err());
    }

    #[test]
    fn created_at_gt_fails_when_less() {
        assert!(evaluate_created_at_conditions("created_at>2000", 1000).is_err());
    }

    #[test]
    fn compound_conditions_all_pass() {
        assert!(evaluate_created_at_conditions("created_at>500&created_at<2000", 1000).is_ok());
    }

    #[test]
    fn compound_conditions_one_fails() {
        assert!(evaluate_created_at_conditions("created_at>500&created_at<900", 1000).is_err());
    }

    #[test]
    fn kind_condition_is_skipped() {
        // kind= clauses must not cause failure — they are skipped per spec
        assert!(evaluate_created_at_conditions("kind=9&created_at>500", 1000).is_ok());
    }

    #[test]
    fn malformed_threshold_returns_err() {
        assert!(evaluate_created_at_conditions("created_at<notanumber", 1000).is_err());
    }
}
