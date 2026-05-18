//! Reproduction probe for the Pocket TTS first-phoneme-drop fix.
//!
//! Pocket TTS' FlowLM has an autoregressive cold-start that occasionally
//! smears or drops the first phoneme of short utterances — see
//! kyutai-labs/pocket-tts #91, #70 and sherpa-onnx #3180. The cure is to
//! prepend a sacrificial `". . "` cold-start absorber to short prompts
//! and trim the resulting leading audio. This example reproduces both
//! variants of generated audio so you can listen-test the fix at the
//! exact `GenerationConfig` we ship in production (`huddle::pocket`):
//!
//!   - silence_scale: 0.0     (production)
//!   - max_frames:    100     (short) / sherpa default 500 (long)
//!   - num_steps: 1
//!   - speed: 1.05
//!
//! Note: production does NOT override `frames_after_eos` — sherpa-onnx's
//! default of 3 is what we want. The previous attempt to override it for
//! long prompts caused the "first 'yep' is static" regression (commit
//! 1dbfa2c). This probe mirrors that decision.
//!
//! Run:
//!   cargo run --release --example prod_probe
//!   cargo run --release --example prod_probe /path/to/pocket-tts
//!
//! Output: /tmp/prod_<label>_s<seed>.wav. The "no sacrificial" variants
//! show what production produced before the fix; the "_sac" variants show
//! the new path. Listen back with `afplay`.

use std::collections::HashMap;
use std::path::PathBuf;

use sherpa_onnx::{
    self, GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsModelConfig,
    OfflineTtsPocketModelConfig, Wave,
};

const SAMPLE_RATE: u32 = 24_000;
const SHORT_PROMPT_MAX_FRAMES: i32 = 100;

// (label, raw_text_before_prep, sacrificial_prefix_to_add_after_pad)
const TESTS: &[(&str, &str, &str)] = &[
    // Short, previously-failing — sacrificial applied
    ("imhappy_sac", "I'm happy.", ". . "),
    ("imsorry_sac", "I'm sorry.", ". . "),
    ("imready_sac", "I'm ready.", ". . "),
    // Short, previously-OK — sacrificial applied
    ("yep_sac", "Yep.", ". . "),
    ("isee_sac", "I see you.", ". . "),
    // Long — sacrificial NOT applied (per design)
    ("longer_nosac", "Hello, how can I help you today?", ""),
    ("multi_nosac", "Yes, that works. Let me try again.", ""),
];

fn main() {
    let model_dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/pocket-tts-bench".to_string());
    let dir = PathBuf::from(&model_dir);
    let p = |name: &str| dir.join(name).to_string_lossy().into_owned();

    let mut cfg = OfflineTtsConfig::default();
    cfg.model = OfflineTtsModelConfig {
        pocket: OfflineTtsPocketModelConfig {
            lm_main: Some(p("lm_main.int8.onnx")),
            lm_flow: Some(p("lm_flow.int8.onnx")),
            encoder: Some(p("encoder.onnx")),
            decoder: Some(p("decoder.int8.onnx")),
            text_conditioner: Some(p("text_conditioner.onnx")),
            vocab_json: Some(p("vocab.json")),
            token_scores_json: Some(p("token_scores.json")),
            voice_embedding_cache_capacity: 16,
        },
        num_threads: 1,
        debug: false,
        ..Default::default()
    };
    let engine = OfflineTts::create(&cfg).expect("engine create");

    let voice_path = dir.join("reference_sample.wav");
    let wave = Wave::read(voice_path.to_str().unwrap()).expect("voice WAV");
    let ref_samples = wave.samples().to_vec();
    let ref_sr = wave.sample_rate();

    let seeds: &[i32] = &[42, 1337, 99999, 7, 314159];

    println!(
        "{:18} | {:>6} | {:>7} | gap_search (50ms,0.02) | path",
        "test", "seed", "len_ms"
    );
    println!("{}", "-".repeat(100));

    for (label, raw_text, sacrificial) in TESTS {
        // Mirror huddle::pocket::prepare_pocket_prompt:
        //   - cleaned text starts with capital, ends with punctuation
        //   - short (≤4 words) → pad + sacrificial; max_frames=100
        //   - long  → unchanged; no max_frames override
        let cleaned = raw_text.trim();
        let word_count = cleaned.split_whitespace().count();
        let is_short = word_count <= 4;
        let pad = if is_short { "        " } else { "" };
        let prompt = format!("{pad}{sacrificial}{cleaned}");

        for seed in seeds {
            let mut extra: HashMap<String, serde_json::Value> = HashMap::new();
            extra.insert("seed".to_string(), serde_json::Value::from(*seed));
            if is_short {
                extra.insert(
                    "max_frames".to_string(),
                    serde_json::Value::from(SHORT_PROMPT_MAX_FRAMES),
                );
            }
            let gen = GenerationConfig {
                speed: 1.05,
                num_steps: 1,
                silence_scale: 0.0, // PRODUCTION SETTING
                reference_audio: Some(ref_samples.clone()),
                reference_sample_rate: ref_sr,
                extra: Some(extra),
                ..Default::default()
            };
            let audio = engine
                .generate_with_config(&prompt, &gen, None::<fn(&[f32], f32) -> bool>)
                .expect("synth");
            let samples = audio.samples();
            let len_ms = samples.len() as f32 / SAMPLE_RATE as f32 * 1000.0;
            let gap = find_gap(samples, SAMPLE_RATE, 0.02, 50);
            let path = format!("/tmp/prod_{}_s{}.wav", label, seed);
            sherpa_onnx::write(&path, samples, SAMPLE_RATE as i32);
            println!(
                "{:18} | {:>6} | {:>5.0}ms | {:>22} | {}",
                label, seed, len_ms, gap, path
            );
        }
        println!();
    }
}

fn find_gap(samples: &[f32], sr: u32, thresh: f32, min_ms: u32) -> String {
    let scan_start = (sr as usize * 30) / 1000;
    let min_samples = (sr as usize * min_ms as usize) / 1000;
    let mut silence_from: Option<usize> = None;
    for i in scan_start..samples.len() {
        if samples[i].abs() < thresh {
            silence_from.get_or_insert(i);
        } else if let Some(start) = silence_from {
            if i - start >= min_samples {
                return format!(
                    "{:.0}..{:.0}ms ({:.0}ms)",
                    start as f32 / sr as f32 * 1000.0,
                    i as f32 / sr as f32 * 1000.0,
                    (i - start) as f32 / sr as f32 * 1000.0
                );
            }
            silence_from = None;
        }
    }
    "<no gap>".to_string()
}
