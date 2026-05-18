//! Pocket TTS engine wrapper around sherpa-onnx's `OfflineTts`.
//!
//! Pocket TTS is a small (~189 MB int8 ONNX) zero-shot voice-cloning TTS
//! model from Kyutai. It runs quickly on CPU via sherpa-onnx, replacing the
//! previous Kokoro-82M engine that also required an espeak-free but
//! lexicon-heavy G2P pipeline (Misaki + CMUdict).
//!
//! ## Attribution
//!
//! - **Model**: Kyutai *Pocket TTS* — Charles, Roebel, et al., 2026.
//!   arXiv:2509.06926. Original repository: <https://huggingface.co/kyutai/pocket-tts>.
//!   Licensed CC-BY-4.0.
//! - **Mimi neural codec**: Kyutai, bundled in the same release. CC-BY-4.0.
//! - **ONNX export**: KevinAHM —
//!   <https://huggingface.co/KevinAHM/pocket-tts-onnx>. CC-BY-4.0.
//! - **sherpa-onnx repackage**: csukuangfj / k2-fsa —
//!   <https://huggingface.co/csukuangfj2/sherpa-onnx-pocket-tts-int8-2026-01-26>.
//!   Repackages KevinAHM's export with the file layout sherpa-onnx's
//!   `OfflineTtsPocketModelConfig` expects. CC-BY-4.0.
//! - **Reference voice WAV** (`reference_sample.wav`): the "Mary
//!   (f, conversation)" preset from the Kyutai TTS demo
//!   (<https://kyutai.org/tts>), which maps to `vctk/p333_023_enhanced.wav`
//!   in <https://huggingface.co/kyutai/tts-voices>. CC-BY-4.0, base recording
//!   from the VCTK corpus, enhanced by ai-coustics.
//!
//! Sprout ships these files unmodified; see the on-disk `MODEL_LICENSE.txt`
//! sidecar written by `huddle::models` during install for the canonical
//! CC-BY-4.0 §3(a)(1) attribution block.
//!
//! ## Engine-module contract (see `huddle::tts`)
//!
//! `pocket.rs` exposes a fixed surface used by `tts.rs`. Mirroring this
//! contract is what lets the TTS pipeline stay engine-agnostic:
//!
//! - `SAMPLE_RATE: u32`             — engine output sample rate in Hz.
//! - `DEFAULT_VOICE: &str`          — default voice name (without extension).
//! - `VOICE_FILE_EXT: &str`         — extension for per-voice files on disk.
//! - `load_text_to_speech(model_dir)`              → `Result<Engine, String>`
//! - `load_voice_style(path)`                      → `Result<VoiceStyle, String>`
//! - `Engine::synth_chunk(&self, text, lang, &VoiceStyle, steps, speed)`
//!                                                 → `Result<Vec<f32>, String>`
//!
//! `lang` and `steps` are accepted for API compatibility with the previous
//! Kokoro engine but are unused — Pocket TTS does its own language ID from
//! the input text and is not a diffusion model (consistency LM, one step).

use std::path::{Path, PathBuf};

use sherpa_onnx::{GenerationConfig, OfflineTts, OfflineTtsConfig, Wave};

// ── Engine-module contract: public consts ─────────────────────────────────────

/// Pocket TTS emits 24 kHz mono PCM. Matches the previous Kokoro output rate,
/// so the rodio sink and inter-sentence silence buffer in `tts.rs` remain valid.
pub const SAMPLE_RATE: u32 = 24_000;

/// Name (without extension) of the bundled reference voice. The model directory
/// is expected to contain `<DEFAULT_VOICE>.<VOICE_FILE_EXT>` after install.
pub const DEFAULT_VOICE: &str = "reference_sample";

/// Voice files for Pocket TTS are reference audio (WAV). Distinct from the
/// Kokoro `.bin` style vectors — the model conditions on raw waveform samples,
/// not a precomputed embedding, so the extension change is honest.
pub const VOICE_FILE_EXT: &str = "wav";

// ── Tuning ────────────────────────────────────────────────────────────────────

/// Single-threaded ONNX execution for predictable CPU contention with the STT
/// pipeline. Matches `STT_NUM_THREADS` in `stt.rs`; raise only if a benchmark
/// argues for it.
const TTS_NUM_THREADS: i32 = 1;

/// LRU cache size for cloned voice embeddings inside the sherpa-onnx engine.
/// We bind to one voice per pipeline today, but the upstream example uses 16
/// and the cost is negligible — keep room for future multi-voice support.
const VOICE_EMBEDDING_CACHE_CAPACITY: i32 = 16;

/// Pocket TTS is a consistency-based LM. Generation quality saturates at one
/// denoising step — the upstream `GenerationConfig` default of 5 multiplies
/// synthesis time by ~5× with no audible benefit on this model.
const SYNTH_NUM_STEPS: i32 = 1;

/// Disable the upstream default 200 ms of pre/post silence padding. We splice
/// `INTER_SENTENCE_SILENCE` in `tts.rs` ourselves and don't want a double
/// helping of leading silence on every utterance.
const SYNTH_SILENCE_SCALE: f32 = 0.0;

// ── ONNX file names (five Pocket TTS sessions plus two JSON tables) ───────────

const FILE_LM_MAIN: &str = "lm_main.int8.onnx";
const FILE_LM_FLOW: &str = "lm_flow.int8.onnx";
const FILE_ENCODER: &str = "encoder.onnx";
const FILE_DECODER: &str = "decoder.int8.onnx";
const FILE_TEXT_COND: &str = "text_conditioner.onnx";
const FILE_VOCAB: &str = "vocab.json";
const FILE_TOKEN_SCORES: &str = "token_scores.json";

// ── Voice style ───────────────────────────────────────────────────────────────

/// Loaded reference voice — normalised f32 PCM samples plus their sample rate.
///
/// Pocket TTS takes a reference waveform per generation call (not a
/// precomputed style embedding), so we keep the samples in memory and clone
/// the small `Vec` into each `GenerationConfig` rather than re-reading the
/// WAV from disk on every sentence.
#[derive(Debug, Clone)]
pub struct VoiceStyle {
    samples: Vec<f32>,
    sample_rate: i32,
}

/// Load a reference voice WAV from disk.
///
/// Accepts any sample rate sherpa-onnx's `Wave::read` can decode — Pocket TTS
/// resamples internally using `reference_sample_rate`. The bundled
/// `reference_sample.wav` ("Mary" — VCTK p333, enhanced) is 32 kHz mono.
pub fn load_voice_style(path: &Path) -> Result<VoiceStyle, String> {
    let path_str = path
        .to_str()
        .ok_or_else(|| format!("voice path is not valid UTF-8: {}", path.display()))?;
    let wave = Wave::read(path_str)
        .ok_or_else(|| format!("could not read voice WAV at {}", path.display()))?;
    let samples = wave.samples().to_vec();
    if samples.is_empty() {
        return Err(format!("voice WAV is empty: {}", path.display()));
    }
    Ok(VoiceStyle {
        samples,
        sample_rate: wave.sample_rate(),
    })
}

// ── Engine ────────────────────────────────────────────────────────────────────

/// Pocket TTS engine handle. Cheap to construct (one `OfflineTts::create`
/// call). Owned by the TTS worker thread for the lifetime of a huddle session.
///
/// `OfflineTts` does not implement `Debug`, so we don't derive it here — the
/// pipeline only needs to move the engine into the worker thread and call
/// `synth_chunk` on it, never to print it.
pub struct PocketTts {
    inner: OfflineTts,
}

/// Build the Pocket TTS engine from the model directory installed by
/// `huddle::models`. Returns `Err` if any expected ONNX or JSON file is
/// missing — readiness is normally enforced by `is_tts_ready` upstream, but
/// the check is repeated here so a manually-modified model dir produces a
/// clear error string instead of an opaque sherpa-onnx `None`.
pub fn load_text_to_speech(model_dir: &str) -> Result<PocketTts, String> {
    let dir = PathBuf::from(model_dir);
    for name in [
        FILE_LM_MAIN,
        FILE_LM_FLOW,
        FILE_ENCODER,
        FILE_DECODER,
        FILE_TEXT_COND,
        FILE_VOCAB,
        FILE_TOKEN_SCORES,
    ] {
        let p = dir.join(name);
        if !p.is_file() {
            return Err(format!("missing Pocket TTS file: {}", p.display()));
        }
    }

    let to_str = |name: &str| -> String { dir.join(name).to_string_lossy().into_owned() };

    // Build the config by mutating defaults — mirrors `stt.rs` and stays
    // resilient if sherpa-onnx adds unrelated model-family fields.
    let mut cfg = OfflineTtsConfig::default();
    cfg.model.pocket.lm_main = Some(to_str(FILE_LM_MAIN));
    cfg.model.pocket.lm_flow = Some(to_str(FILE_LM_FLOW));
    cfg.model.pocket.encoder = Some(to_str(FILE_ENCODER));
    cfg.model.pocket.decoder = Some(to_str(FILE_DECODER));
    cfg.model.pocket.text_conditioner = Some(to_str(FILE_TEXT_COND));
    cfg.model.pocket.vocab_json = Some(to_str(FILE_VOCAB));
    cfg.model.pocket.token_scores_json = Some(to_str(FILE_TOKEN_SCORES));
    cfg.model.pocket.voice_embedding_cache_capacity = VOICE_EMBEDDING_CACHE_CAPACITY;
    cfg.model.num_threads = TTS_NUM_THREADS;
    // Explicit — defaults are not part of the API contract, and noisy debug
    // logging in release builds would be expensive on every synthesized chunk.
    cfg.model.debug = false;

    let inner = OfflineTts::create(&cfg)
        .ok_or_else(|| "OfflineTts::create returned None for Pocket TTS".to_string())?;
    Ok(PocketTts { inner })
}

impl PocketTts {
    /// Synthesise `text` with the given reference voice.
    ///
    /// `_lang` and `_steps` are accepted for API compatibility with the
    /// previous Kokoro engine. Pocket TTS infers language from the input text
    /// directly and is a one-step consistency model. Returns an empty buffer
    /// for whitespace-only input.
    pub fn synth_chunk(
        &self,
        text: &str,
        _lang: &str,
        style: &VoiceStyle,
        _steps: usize,
        speed: f32,
    ) -> Result<Vec<f32>, String> {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }

        let cfg = GenerationConfig {
            speed,
            num_steps: SYNTH_NUM_STEPS,
            silence_scale: SYNTH_SILENCE_SCALE,
            reference_audio: Some(style.samples.clone()),
            reference_sample_rate: style.sample_rate,
            ..Default::default()
        };

        // No progress callback — synthesis is fast enough that returning the
        // whole buffer at once keeps the lookahead pipelining in `tts.rs`
        // simple. `None::<fn(...) -> bool>` pins the callback type for the
        // `generate_with_config` generic parameter.
        let audio = self
            .inner
            .generate_with_config(text, &cfg, None::<fn(&[f32], f32) -> bool>)
            .ok_or_else(|| {
                format!(
                    "Pocket TTS synthesis failed for text ({} chars)",
                    text.len()
                )
            })?;

        let sample_rate = audio.sample_rate();
        if sample_rate != SAMPLE_RATE as i32 {
            eprintln!(
                "sprout-desktop: Pocket TTS returned unexpected sample rate {sample_rate}Hz \
                 (expected {SAMPLE_RATE}Hz); playback speed may be wrong"
            );
        }

        Ok(audio.samples().to_vec())
    }
}
