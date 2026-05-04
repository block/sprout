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
    /// Optional session expiry derived from `created_at<T` conditions.
    /// The minimum (most restrictive) `created_at<T` threshold found in the
    /// auth tag conditions, or `None` if no such condition is present.
    pub session_expiry: Option<u64>,
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
/// NIP-OA mandates lowercase hex; `secp256k1::from_hex` silently accepts
/// uppercase, so we enforce the spec constraint here before the crypto call.
///
/// Tags with fewer than 4 elements are passed through and will fail the
/// `verify_auth_tag` element-count check instead.
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
/// ## Performance note — rate-limit re-auth attempts
///
/// This function performs CPU-intensive Schnorr signature verification (offloaded
/// to `spawn_blocking`). Callers **must** apply a per-IP or per-pubkey rate limit
/// on re-authentication attempts to prevent CPU exhaustion. The rate limiting is
/// implemented in `auth.rs`, not here.
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
/// | `kind=` condition present | `Err` | `"restricted: unsupported condition for NIP-AA: kind= restrictions are not yet enforced per-action…"` |
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
        None => return Ok(None),
        Some(tag) => tag,
    };
    // Step 4: Pre-validate lowercase hex before the crypto call.
    validate_auth_tag_hex(auth_tag)?;

    let tag_json = serde_json::to_string(&auth_tag.as_slice())
        .map_err(|e| format!("restricted: failed to serialize auth tag: {e}"))?;

    // Step 4: Verify the auth tag — CPU-intensive, run on blocking thread pool.
    let agent_pk_owned = *agent_pubkey;
    let tag_json_owned = tag_json.clone();
    let owner_pubkey = match tokio::task::spawn_blocking(move || {
        sprout_sdk::nip_oa::verify_auth_tag(&tag_json_owned, &agent_pk_owned)
    })
    .await
    {
        Ok(Ok(pk)) => pk,
        Ok(Err(e)) => {
            return Err(format!("restricted: invalid auth tag: {e}"));
        }
        Err(e) => {
            tracing::warn!(error = %e, "NIP-AA: verify_auth_tag task panicked");
            return Err("restricted: verification failed".to_string());
        }
    };

    // Step 4b: Evaluate created_at conditions from the auth tag (element [2]).
    let tag_slice = auth_tag.as_slice();
    let session_expiry = if tag_slice.len() >= 3 {
        let conditions = &tag_slice[2];
        if !conditions.is_empty() {
            if let Err(reason) = evaluate_created_at_conditions(conditions, event_created_at) {
                return Err(format!("restricted: {reason}"));
            }
            extract_session_expiry(conditions)
        } else {
            None
        }
    } else {
        None
    };

    // Step 5: Check owner is an active relay member. Fail closed on DB errors.
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

    Ok(Some(NipAaResult {
        owner_pubkey,
        session_expiry,
    }))
}

/// Extract the minimum `created_at<T` threshold from a conditions string.
///
/// Returns the smallest (most restrictive) upper-bound threshold found, or `None`
/// if no `created_at<T` condition is present. Used to derive session expiry for
/// NIP-AA virtual members so the relay can enforce the time bound post-login.
pub fn extract_session_expiry(conditions: &str) -> Option<u64> {
    if conditions.is_empty() {
        return None;
    }
    let mut min_expiry: Option<u64> = None;
    for clause in conditions.split('&') {
        if let Some(val_str) = clause.strip_prefix("created_at<") {
            if let Ok(threshold) = val_str.parse::<u64>() {
                min_expiry = Some(match min_expiry {
                    Some(prev) => prev.min(threshold),
                    None => threshold,
                });
            }
        }
    }
    min_expiry
}

/// Evaluate `created_at<t` and `created_at>t` conditions against an event's created_at.
/// Returns `Ok(())` if all conditions pass, `Err(reason)` if any fail.
///
/// ## `kind=` conditions are rejected (fail-closed)
///
/// `kind=` clauses restrict which event kinds the agent may publish. We cannot
/// enforce these per-action yet — silently skipping them would grant broader
/// access than the owner intended, which is a privilege escalation vector.
/// Until per-action enforcement is implemented, we reject any auth tag that
/// contains a `kind=` clause so that owner intent is always honoured.
///
/// ## Other constraints
///
/// Per NIP-OA spec: verifiers MUST reject an auth tag that contains an unsupported
/// clause. Empty clauses (from leading/trailing `&`) and unknown clause types are
/// therefore rejected with an error.
fn evaluate_created_at_conditions(conditions: &str, event_created_at: u64) -> Result<(), String> {
    if conditions.is_empty() {
        return Ok(());
    }
    for clause in conditions.split('&') {
        if clause.is_empty() {
            return Err("malformed conditions string (empty clause)".to_string());
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
        } else if clause.starts_with("kind=") {
            // kind= conditions restrict which event kinds the agent may publish.
            // We cannot enforce these per-action yet — silently skipping them would
            // grant broader access than the owner intended (privilege escalation).
            // Reject fail-closed until per-action enforcement is implemented.
            return Err(format!(
                "unsupported condition for NIP-AA: kind= restrictions are not yet enforced \
                 per-action; rejecting to prevent privilege escalation (clause: {clause})"
            ));
        } else {
            return Err(format!("unsupported condition clause: {clause}"));
        }
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
    fn kind_condition_is_rejected() {
        // kind= clauses must be rejected fail-closed to prevent privilege escalation
        assert!(evaluate_created_at_conditions("kind=9&created_at>500", 1000).is_err());
    }

    #[test]
    fn kind_condition_alone_is_rejected() {
        let result = evaluate_created_at_conditions("kind=9", 1000);
        assert!(result.is_err(), "expected Err for kind= condition, got Ok");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("kind="),
            "expected error to mention kind=, got: {msg}"
        );
    }

    #[test]
    fn kind_and_created_at_conditions_rejected_due_to_kind() {
        // The kind= clause causes rejection even when created_at would pass.
        let result = evaluate_created_at_conditions("kind=9&created_at>500", 1000);
        assert!(
            result.is_err(),
            "expected Err because of kind= clause, got Ok"
        );
    }

    #[test]
    fn malformed_threshold_returns_err() {
        assert!(evaluate_created_at_conditions("created_at<notanumber", 1000).is_err());
    }

    // ── extract_single_auth_tag ───────────────────────────────────────────────

    #[test]
    fn no_auth_tags_returns_ok_none() {
        let tags: Vec<nostr::Tag> = vec![];
        assert!(matches!(extract_single_auth_tag(&tags), Ok(None)));
    }

    #[test]
    fn non_auth_tags_ignored_returns_ok_none() {
        let tags = vec![
            nostr::Tag::parse(&["p", "deadbeef"]).expect("valid tag"),
            nostr::Tag::parse(&["e", "cafebabe"]).expect("valid tag"),
        ];
        assert!(matches!(extract_single_auth_tag(&tags), Ok(None)));
    }

    #[test]
    fn multiple_auth_tags_returns_err() {
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
        let owner = make_hex(64, true); // uppercase — secp256k1 accepts it, NIP-OA forbids it
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
        // Short tags pass here and fail later in verify_auth_tag's element-count check.
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
        assert!(evaluate_created_at_conditions("created_at<1001", 1000).is_ok());
    }

    #[test]
    fn created_at_gt_passes_one_above_boundary() {
        assert!(evaluate_created_at_conditions("created_at>999", 1000).is_ok());
    }

    #[test]
    fn multiple_kind_conditions_are_all_rejected() {
        // Each kind= clause triggers a rejection; the first one encountered returns Err.
        assert!(
            evaluate_created_at_conditions("kind=9&kind=1&kind=30023&created_at>0", 1000).is_err()
        );
    }

    #[test]
    fn empty_clause_from_leading_ampersand_is_rejected() {
        assert!(evaluate_created_at_conditions("&created_at>0", 1000).is_err());
    }

    #[test]
    fn empty_clause_from_trailing_ampersand_is_rejected() {
        assert!(evaluate_created_at_conditions("created_at>0&", 1000).is_err());
    }

    #[test]
    fn unknown_clause_type_is_rejected() {
        // NIP-OA: verifiers MUST reject auth tags with unsupported clauses.
        assert!(evaluate_created_at_conditions("foo=bar&created_at>0", 1000).is_err());
    }

    // ── self-attestation rejection ────────────────────────────────────────────

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
