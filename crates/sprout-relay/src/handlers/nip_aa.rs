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

/// Extract exactly one `auth` tag from a slice of event tags.
///
/// - Returns `Ok(None)` if no `auth` tag is present (not a NIP-AA attempt).
/// - Returns `Err` if more than one `auth` tag is present (ambiguous credential).
/// - Returns `Ok(Some(&tag))` for exactly one match.
///
/// Extracted as a pure function so it can be unit-tested without `AppState`.
fn extract_single_auth_tag(tags: &[nostr::Tag]) -> Result<Option<&nostr::Tag>, String> {
    let auth_tags: Vec<&nostr::Tag> = tags
        .iter()
        .filter(|t| {
            let s = t.as_slice();
            !s.is_empty() && s[0] == "auth"
        })
        .collect();

    match auth_tags.len() {
        0 => Ok(None),
        1 => Ok(Some(auth_tags[0])),
        _ => Err("restricted: multiple auth tags present".to_string()),
    }
}

/// Extract and verify a NIP-OA auth tag from event tags for NIP-AA authentication.
///
/// Implements NIP-AA Steps 3-5:
/// - Step 3: Extract exactly one `auth` tag (zero → Ok(None); >1 → Err)
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
    let auth_tag = match extract_single_auth_tag(tags)? {
        None => return Ok(None), // No auth tag — not an agent, not a NIP-AA attempt
        Some(tag) => tag,
    };
    // Step 4: Pre-validate lowercase hex requirements before calling verify_auth_tag.
    // NIP-OA requires 64-char lowercase hex owner pubkey and 128-char lowercase hex sig.
    // secp256k1's from_hex accepts uppercase, so we enforce the spec constraint here.
    {
        let tag_slice = auth_tag.as_slice();
        if tag_slice.len() >= 4 {
            let owner_hex = &tag_slice[1];
            if owner_hex.len() != 64
                || !owner_hex
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            {
                return Err(format!(
                    "restricted: owner pubkey must be 64 lowercase hex chars, got {:?}",
                    owner_hex
                ));
            }
            let sig_hex = &tag_slice[3];
            if sig_hex.len() != 128
                || !sig_hex
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            {
                return Err(format!(
                    "restricted: signature must be 128 lowercase hex chars, got length {}",
                    sig_hex.len()
                ));
            }
        }
    }

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

    // Step 5: Check owner is an active relay member.
    // Log DB errors server-side; send a sanitized fail-closed message to the client
    // to avoid leaking internal error details.
    let owner_hex = owner_pubkey.to_hex();
    let is_member = match state.db.is_relay_member(&owner_hex).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                agent = %agent_pubkey.to_hex(),
                owner = %owner_hex,
                error = %e,
                "NIP-AA: owner membership DB check failed"
            );
            return Err("restricted: membership check failed".to_string());
        }
    };

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

    // ── extract_single_auth_tag ───────────────────────────────────────────────
    // These tests cover the NIP-AA §Step 3 logic (no auth tags → Ok(None),
    // multiple auth tags → Err) without requiring AppState or async runtime.

    #[test]
    fn no_auth_tags_returns_ok_none() {
        // verify_nip_aa returns Ok(None) when no auth tag is present — the
        // caller treats this as "not an agent" and falls through to a plain
        // membership failure.
        let tags: Vec<nostr::Tag> = vec![];
        assert!(matches!(extract_single_auth_tag(&tags), Ok(None)));
    }

    #[test]
    fn non_auth_tags_ignored_returns_ok_none() {
        // Unrelated tags (e.g. "p", "e") must not be mistaken for auth tags.
        let tags = vec![
            nostr::Tag::parse(&["p", "deadbeef"]).expect("valid tag"),
            nostr::Tag::parse(&["e", "cafebabe"]).expect("valid tag"),
        ];
        assert!(matches!(extract_single_auth_tag(&tags), Ok(None)));
    }

    #[test]
    fn multiple_auth_tags_returns_err() {
        // NIP-AA §Step 3: more than one auth tag is ambiguous and must be
        // rejected. verify_nip_aa propagates this as Err.
        let tags = vec![
            nostr::Tag::parse(&["auth", "owner1hex", "", "sig1"]).expect("valid tag"),
            nostr::Tag::parse(&["auth", "owner2hex", "", "sig2"]).expect("valid tag"),
        ];
        let result = extract_single_auth_tag(&tags);
        assert!(
            result.is_err(),
            "expected Err for multiple auth tags, got {result:?}"
        );
        assert!(result.unwrap_err().contains("multiple auth tags"));
    }

    #[test]
    fn single_auth_tag_returns_ok_some() {
        let tags = vec![
            nostr::Tag::parse(&["auth", "ownerhex", "created_at>0", "sig"]).expect("valid tag"),
        ];
        assert!(matches!(extract_single_auth_tag(&tags), Ok(Some(_))));
    }
}
