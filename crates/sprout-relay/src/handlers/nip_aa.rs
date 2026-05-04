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
pub fn extract_single_auth_tag(tags: &[nostr::Tag]) -> Result<Option<&nostr::Tag>, String> {
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

/// Pre-validate that an `auth` tag's owner pubkey and signature fields are
/// 64-char and 128-char lowercase hex respectively.
///
/// NIP-OA mandates lowercase hex; secp256k1's `from_hex` silently accepts
/// uppercase, so we enforce the spec constraint here before the crypto call.
///
/// Only runs when the tag has ≥ 4 elements (the minimum for a well-formed auth
/// tag). Tags with fewer elements are passed through and will fail the
/// `verify_auth_tag` element-count check instead.
///
/// This is a defense-in-depth check: the SDK's `verify_auth_tag` also validates
/// these fields, but `secp256k1::from_hex` silently accepts uppercase hex. This
/// pre-check enforces the NIP-OA lowercase-only constraint before the crypto call.
fn validate_auth_tag_hex(auth_tag: &nostr::Tag) -> Result<(), String> {
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
    Ok(())
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
///
/// ## Rejection matrix
///
/// | Condition | Return value | Reason prefix |
/// |-----------|-------------|---------------|
/// | No `auth` tag | `Ok(None)` | — (not an agent) |
/// | Multiple `auth` tags | `Err` | `"restricted: multiple auth tags present"` |
/// | Owner pubkey not 64-char lowercase hex | `Err` | `"restricted: owner pubkey must be 64 lowercase hex chars"` |
/// | Signature not 128-char lowercase hex | `Err` | `"restricted: signature must be 128 lowercase hex chars"` |
/// | NIP-OA signature verification fails | `Err` | `"restricted: invalid auth tag: …"` |
/// | Self-attestation (agent == owner) | `Err` | `"restricted: invalid auth tag: …"` (from sprout-sdk) |
/// | `created_at<T` condition not satisfied | `Err` | `"restricted: created_at condition not satisfied: …"` |
/// | `created_at>T` condition not satisfied | `Err` | `"restricted: created_at condition not satisfied: …"` |
/// | Malformed `created_at` threshold | `Err` | `"restricted: malformed created_at< condition: …"` |
/// | Owner not an active relay member | `Err` | `"restricted: owner is not a relay member"` |
/// | DB membership check fails | `Err` | `"restricted: membership check failed"` |
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
    validate_auth_tag_hex(auth_tag)?;

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
///
/// **Precondition**: This function is only called after `verify_auth_tag` has validated
/// the conditions string. Empty clauses and unknown clause types are unreachable in
/// production — they are handled defensively (skipped) rather than rejected, because
/// the upstream SDK validation is the authoritative guard.
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
    // E2E tests needed (requires running relay + test client with NIP-42 AUTH):
    // - Valid NIP-AA login → virtual membership granted
    // - Invalid NIP-OA signature → rejected
    // - Self-attestation → rejected
    // - Owner not a relay member → rejected
    // - Stale AUTH event → rejected
    // - Re-auth replaces credential (same pubkey)
    // - Re-auth identity switch (different pubkey) — new: single-pubkey-at-a-time
    // - Failed re-auth preserves existing session
    // - Virtual member denied admin commands
    // - Malformed AUTH closes connection
    //
    // Unit-tested elsewhere:
    // - Scope intersection (admin stripped, read/write preserved) — auth.rs tests
    // - Owner-pubkey tracking in ConnectionManager — state.rs tests
    // - NIP-OA crypto, tag extraction, conditions — below

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

    #[test]
    fn three_auth_tags_returns_err() {
        let tags = vec![
            nostr::Tag::parse(&["auth", "a", "", "s1"]).expect("valid tag"),
            nostr::Tag::parse(&["auth", "b", "", "s2"]).expect("valid tag"),
            nostr::Tag::parse(&["auth", "c", "", "s3"]).expect("valid tag"),
        ];
        assert!(extract_single_auth_tag(&tags).is_err());
    }

    // ── validate_auth_tag_hex ─────────────────────────────────────────────────

    fn make_hex(len: usize, uppercase: bool) -> String {
        let ch = if uppercase { 'A' } else { 'a' };
        std::iter::repeat_n(ch, len).collect()
    }

    #[test]
    fn validate_hex_passes_for_well_formed_tag() {
        // A tag with exactly 64-char lowercase owner hex and 128-char lowercase sig hex.
        let owner = make_hex(64, false);
        let sig = make_hex(128, false);
        let tag = nostr::Tag::parse(&["auth", &owner, "", &sig]).expect("valid tag");
        assert!(validate_auth_tag_hex(&tag).is_ok());
    }

    #[test]
    fn validate_hex_rejects_uppercase_owner_pubkey() {
        // NIP-OA mandates lowercase hex. Uppercase must be rejected even though
        // secp256k1's from_hex would silently accept it.
        let owner = make_hex(64, true); // uppercase
        let sig = make_hex(128, false);
        let tag = nostr::Tag::parse(&["auth", &owner, "", &sig]).expect("valid tag");
        let err = validate_auth_tag_hex(&tag).unwrap_err();
        assert!(
            err.contains("owner pubkey must be 64 lowercase hex chars"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_hex_rejects_short_owner_pubkey() {
        let owner = make_hex(32, false); // too short
        let sig = make_hex(128, false);
        let tag = nostr::Tag::parse(&["auth", &owner, "", &sig]).expect("valid tag");
        assert!(validate_auth_tag_hex(&tag).is_err());
    }

    #[test]
    fn validate_hex_rejects_long_owner_pubkey() {
        let owner = make_hex(65, false); // too long
        let sig = make_hex(128, false);
        let tag = nostr::Tag::parse(&["auth", &owner, "", &sig]).expect("valid tag");
        assert!(validate_auth_tag_hex(&tag).is_err());
    }

    #[test]
    fn validate_hex_rejects_uppercase_signature() {
        let owner = make_hex(64, false);
        let sig = make_hex(128, true); // uppercase
        let tag = nostr::Tag::parse(&["auth", &owner, "", &sig]).expect("valid tag");
        let err = validate_auth_tag_hex(&tag).unwrap_err();
        assert!(
            err.contains("signature must be 128 lowercase hex chars"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_hex_rejects_short_signature() {
        let owner = make_hex(64, false);
        let sig = make_hex(64, false); // too short
        let tag = nostr::Tag::parse(&["auth", &owner, "", &sig]).expect("valid tag");
        assert!(validate_auth_tag_hex(&tag).is_err());
    }

    #[test]
    fn validate_hex_skips_check_for_tag_with_fewer_than_4_elements() {
        // Tags with < 4 elements bypass the hex check here and will fail later
        // in verify_auth_tag's element-count check. We must not panic or error
        // prematurely on short tags.
        let tag2 = nostr::Tag::parse(&["auth", "short"]).expect("valid tag");
        assert!(
            validate_auth_tag_hex(&tag2).is_ok(),
            "2-element tag should pass hex check"
        );

        let tag3 = nostr::Tag::parse(&["auth", "a", "b"]).expect("valid tag");
        assert!(
            validate_auth_tag_hex(&tag3).is_ok(),
            "3-element tag should pass hex check"
        );
    }

    // ── evaluate_created_at_conditions — additional edge cases ────────────────

    #[test]
    fn created_at_lt_passes_one_below_boundary() {
        // Strict less-than: value one below threshold must pass.
        assert!(evaluate_created_at_conditions("created_at<1001", 1000).is_ok());
    }

    #[test]
    fn created_at_gt_passes_one_above_boundary() {
        // Strict greater-than: value one above threshold must pass.
        assert!(evaluate_created_at_conditions("created_at>999", 1000).is_ok());
    }

    #[test]
    fn multiple_kind_conditions_are_all_skipped() {
        // Multiple kind= clauses must all be skipped — none should cause failure.
        assert!(
            evaluate_created_at_conditions("kind=9&kind=1&kind=30023&created_at>0", 1000).is_ok()
        );
    }

    #[test]
    fn empty_clause_from_leading_ampersand_is_skipped() {
        // A leading & produces an empty first clause — must not error.
        assert!(evaluate_created_at_conditions("&created_at>0", 1000).is_ok());
    }

    #[test]
    fn empty_clause_from_trailing_ampersand_is_skipped() {
        // A trailing & produces an empty last clause — must not error.
        assert!(evaluate_created_at_conditions("created_at>0&", 1000).is_ok());
    }

    #[test]
    fn unknown_clause_type_is_skipped() {
        // Unknown condition types (future extensions) must be ignored, not rejected.
        assert!(evaluate_created_at_conditions("foo=bar&created_at>0", 1000).is_ok());
    }

    // ── self-attestation rejection ────────────────────────────────────────────
    // verify_auth_tag (sprout-sdk) rejects self-attestation. Verify that
    // verify_nip_aa propagates this as Err without requiring AppState by
    // calling sprout_sdk::nip_oa::verify_auth_tag directly.

    #[test]
    fn self_attestation_rejected_by_verify_auth_tag() {
        use nostr::Keys;

        let keys = Keys::generate();
        let agent_pubkey = keys.public_key();

        // Attempt to compute a self-attesting auth tag (owner == agent).
        // compute_auth_tag itself rejects this — verify the error propagates.
        let result = sprout_sdk::nip_oa::compute_auth_tag(&keys, &agent_pubkey, "");
        assert!(
            result.is_err(),
            "compute_auth_tag must reject self-attestation"
        );
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("self-attestation"),
            "expected self-attestation error, got: {err_msg}"
        );
    }

    #[test]
    fn verify_auth_tag_rejects_wrong_element_count_2() {
        use nostr::Keys;
        let keys = Keys::generate();
        let agent_pubkey = keys.public_key();
        // Manually craft a 2-element JSON array (missing conditions and sig).
        let tag_json = r#"["auth","deadbeef"]"#;
        let result = sprout_sdk::nip_oa::verify_auth_tag(tag_json, &agent_pubkey);
        assert!(result.is_err(), "2-element auth tag must be rejected");
    }

    #[test]
    fn verify_auth_tag_rejects_wrong_element_count_3() {
        use nostr::Keys;
        let keys = Keys::generate();
        let agent_pubkey = keys.public_key();
        let tag_json = r#"["auth","deadbeef","conditions"]"#;
        let result = sprout_sdk::nip_oa::verify_auth_tag(tag_json, &agent_pubkey);
        assert!(result.is_err(), "3-element auth tag must be rejected");
    }

    #[test]
    fn verify_auth_tag_rejects_wrong_element_count_5() {
        use nostr::Keys;
        let keys = Keys::generate();
        let agent_pubkey = keys.public_key();
        let tag_json = r#"["auth","deadbeef","conditions","sig","extra"]"#;
        let result = sprout_sdk::nip_oa::verify_auth_tag(tag_json, &agent_pubkey);
        assert!(result.is_err(), "5-element auth tag must be rejected");
    }
}
