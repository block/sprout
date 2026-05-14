//! Multi-profile wrapper layer (plaintext `schema_version >= 3`).
//!
//! Today's single-profile envelope (`StoredSettings`) becomes one slot
//! inside a `ProfilesPlaintext { default_profile_id, profiles }` wrapper.
//! Legacy plaintexts (`schema_version 1`/`2`) are migrated idempotently
//! on first read by `parse_or_migrate_plaintext`.
//!
//! Migration rules:
//! - v1 (`owner_pubkey` empty) → stamp it with envelope.pubkey, bump
//!   `schema_version` to 2 inside the slot, wrap as the sole profile,
//!   mark it as default, label "Default".
//! - v2 with matching `owner_pubkey` → same wrap as v1, no field rewrite.
//! - v2 with mismatched `owner_pubkey` → abort (caller surfaces as
//!   IdentityMismatch). Refusing to "launder" a mismatched plaintext into
//!   a clean v3 envelope under the current pubkey.
//!
//! The wrapper-level `owner_pubkey` is the authoritative identity check;
//! per-profile `settings.owner_pubkey` continues to drive the v2 cross-
//! check exercised in `load_for_spawn`.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use nostr::Keys;
use zeroize::Zeroizing;

use super::storage::{encrypt_settings, write_envelope};
use super::{
    NamedProfile, ProfilesPlaintext, SchemaProbe, SettingsEnvelope, StoredSettings,
    CURRENT_PLAINTEXT_SCHEMA, ENVELOPE_ALG, ENVELOPE_VERSION, MAX_KNOWN_PLAINTEXT_SCHEMA,
};

/// Max user-visible profile label length (after trimming).
pub(super) const MAX_LABEL_BYTES: usize = 64;

/// Validate and normalize a user-supplied profile label. Trims whitespace,
/// rejects empty / control-chars / over-length.
pub(super) fn validate_label(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("label must not be empty".into());
    }
    if trimmed.len() > MAX_LABEL_BYTES {
        return Err(format!(
            "label too long ({} bytes > {MAX_LABEL_BYTES} cap)",
            trimmed.len()
        ));
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err("label must not contain control characters".into());
    }
    Ok(trimmed.to_owned())
}

/// Outcome of decrypting + parsing a plaintext payload.
pub(super) enum ParsedPlaintext {
    /// Plaintext was already v3 — no migration needed.
    Current(ProfilesPlaintext),
    /// Plaintext was v1/v2 — we built a `ProfilesPlaintext` from a single
    /// migrated profile. Caller MAY persist this back to disk (spawn path
    /// does best-effort; commands-side reads MUST persist or fail loudly).
    Migrated(ProfilesPlaintext),
}

/// Typed failure cases from `parse_or_migrate_plaintext`. We split out
/// `IdentityMismatch` from generic `Other` so call sites do not have to
/// string-match an error message to decide whether to surface to the UI
/// as an identity-rotation banner or as a load error.
#[derive(Debug)]
pub(super) enum ParseError {
    /// Embedded `owner_pubkey` (either the v3 wrapper or the legacy
    /// `StoredSettings`) does not match the envelope pubkey. Same threat
    /// model as `envelope.pubkey` mismatch — surface as identity rotation.
    IdentityMismatch,
    /// Anything else: malformed JSON, unsupported schema version, etc.
    /// Carries a human-readable message for surfacing to the UI as a
    /// generic load error.
    Other(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::IdentityMismatch => f.write_str("identity mismatch"),
            ParseError::Other(msg) => f.write_str(msg),
        }
    }
}

/// Parse a decrypted plaintext blob into a `ProfilesPlaintext`, migrating
/// legacy single-profile schemas as needed.
///
/// Identity check: caller must pass the envelope's pubkey; we verify that
/// any embedded `owner_pubkey` (in either the wrapper or the inner legacy
/// `StoredSettings`) matches. On mismatch, returns Err — caller should
/// surface as IdentityMismatch (NOT silently "launder" the plaintext).
pub(super) fn parse_or_migrate_plaintext(
    plain: &str,
    envelope_pubkey: &str,
) -> Result<ParsedPlaintext, ParseError> {
    let probe: SchemaProbe = serde_json::from_str(plain)
        .map_err(|e| ParseError::Other(format!("parse plaintext schema probe: {e}")))?;
    if probe.schema_version > MAX_KNOWN_PLAINTEXT_SCHEMA {
        return Err(ParseError::Other(format!(
            "unsupported plaintext schema_version {} (max known {})",
            probe.schema_version, MAX_KNOWN_PLAINTEXT_SCHEMA
        )));
    }
    if probe.schema_version == 3 {
        let wrapper: ProfilesPlaintext = serde_json::from_str(plain)
            .map_err(|e| ParseError::Other(format!("parse v3 plaintext: {e}")))?;
        if wrapper.owner_pubkey != envelope_pubkey {
            return Err(ParseError::IdentityMismatch);
        }
        return Ok(ParsedPlaintext::Current(wrapper));
    }
    // Legacy schema_version 0/1/2 — parse as bare StoredSettings, then wrap.
    let stored: StoredSettings = serde_json::from_str(plain)
        .map_err(|e| ParseError::Other(format!("parse legacy plaintext: {e}")))?;
    // v2 embeds owner_pubkey for defense-in-depth. v1 has it empty.
    if stored.schema_version >= 2
        && !stored.owner_pubkey.is_empty()
        && stored.owner_pubkey != envelope_pubkey
    {
        return Err(ParseError::IdentityMismatch);
    }
    let now = now_unix();
    let id = new_profile_id();
    let mut migrated = stored;
    // Future readers expect a non-empty owner_pubkey on v2+ slots. Stamp
    // the envelope pubkey for v1 plaintexts.
    if migrated.owner_pubkey.is_empty() {
        migrated.owner_pubkey = envelope_pubkey.to_owned();
    }
    // Bump the inner per-profile schema to 2 so the v2 cross-check stays
    // meaningful on the next read.
    if migrated.schema_version < 2 {
        migrated.schema_version = 2;
    }
    let profile = NamedProfile {
        id: id.clone(),
        label: "Default".into(),
        created_at: now,
        updated_at: now,
        settings: migrated,
    };
    let wrapper = ProfilesPlaintext {
        schema_version: CURRENT_PLAINTEXT_SCHEMA,
        owner_pubkey: envelope_pubkey.to_owned(),
        default_profile_id: Some(id),
        profiles: vec![profile],
    };
    Ok(ParsedPlaintext::Migrated(wrapper))
}

/// Encrypt a `ProfilesPlaintext` and write the envelope atomically.
///
/// The plaintext is serialized into a `Zeroizing<String>` to wipe the bytes
/// after `nip44::encrypt` consumes them. `StoredSettings::Drop` wipes
/// `api_key` on the wrapper-side struct drop; this function adds the
/// JSON-serialized-buffer wipe on top.
pub(super) fn write_profiles_envelope(
    path: &Path,
    keys: &Keys,
    wrapper: &ProfilesPlaintext,
) -> Result<(), String> {
    let plaintext = Zeroizing::new(
        serde_json::to_string(wrapper).map_err(|e| format!("serialize plaintext profiles: {e}"))?,
    );
    let ciphertext = encrypt_settings(keys, &plaintext)?;
    let envelope = SettingsEnvelope {
        version: ENVELOPE_VERSION,
        alg: ENVELOPE_ALG.into(),
        pubkey: keys.public_key().to_hex(),
        ciphertext,
        updated_at: now_unix(),
    };
    write_envelope(path, &envelope)
}

/// Generate a fresh profile id. We use UUID v4 (lowercase hyphenated) —
/// the `uuid` crate is already a direct dep and ULID's lexicographic
/// sortability isn't useful for opaque profile IDs.
pub(super) fn new_profile_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
