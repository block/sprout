//! Unit tests for `mesh_llm/mod.rs` private helpers (kept in a sibling file so
//! `mod.rs` stays under the 500-line budget; `#[path]`-included from there).
use super::{find_progressish_reason, looks_like_model_ref};
use serde_json::json;

#[test]
fn progressish_reads_typed_phase_not_whole_tree() {
    assert_eq!(
        find_progressish_reason(&json!({"phase": "downloading weights"})),
        Some("downloading model".to_string())
    );
    // Regression (Sami N1): an unrelated field mentioning a progress word must
    // not trip the badge — only the typed phase field counts.
    assert_eq!(
        find_progressish_reason(&json!({
            "phase": "ready",
            "model_name": "prepared-qwen-preparing"
        })),
        None
    );
    assert_eq!(find_progressish_reason(&json!({"foo": "bar"})), None);
}

#[test]
fn model_ref_is_family_agnostic() {
    assert!(looks_like_model_ref("hf://org/model"));
    assert!(looks_like_model_ref("some-model.gguf"));
    assert!(looks_like_model_ref("Some-Model.GGUF"));
    // Families that used to be hardcoded must route via the structured path,
    // not a name allowlist here (Sami N2):
    assert!(!looks_like_model_ref("Mistral-7B"));
    assert!(!looks_like_model_ref("Qwen3-35B"));
    assert!(!looks_like_model_ref(""));
}
