//! Supertonic TTS engine — wraps the 4-ONNX-session pipeline from
//! `supertone-inc/supertonic` and exposes a clean `call()` API that returns
//! `Vec<f32>` samples at 44.1 kHz.
//!
//! Mental model:
//!   load_text_to_speech(onnx_dir) → TextToSpeech
//!   load_voice_style(path)        → Style
//!   tts.call(text, lang, &style)  → Vec<f32> @ 44.1 kHz

use ndarray::{Array, Array3};
use rand_distr::{Distribution, Normal};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

use ort::{session::Session, value::Value};

use super::preprocessing::split_sentences;

// ── Public constants ──────────────────────────────────────────────────────────

pub const SAMPLE_RATE: u32 = 44_100;

pub const VOICES: &[&str] = &["F1", "F2", "F3", "F4", "F5", "M1", "M2", "M3", "M4", "M5"];
pub const DEFAULT_VOICE: &str = "F1";

pub const AVAILABLE_LANGS: &[&str] = &["en", "ko", "es", "pt", "fr"];

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Config {
    pub ae: AEConfig,
    pub ttl: TTLConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AEConfig {
    pub sample_rate: i32,
    pub base_chunk_size: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TTLConfig {
    pub chunk_compress_factor: i32,
    pub latent_dim: i32,
}

fn load_cfgs<P: AsRef<Path>>(onnx_dir: P) -> Result<Config, String> {
    let cfg_path = onnx_dir.as_ref().join("tts.json");
    let file = File::open(&cfg_path).map_err(|e| format!("open tts.json: {e}"))?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).map_err(|e| format!("parse tts.json: {e}"))
}

// ── Voice style ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct VoiceStyleData {
    pub style_ttl: StyleComponent,
    pub style_dp: StyleComponent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StyleComponent {
    pub data: Vec<Vec<Vec<f32>>>,
    pub dims: Vec<usize>,
    #[serde(rename = "type")]
    pub dtype: String,
}

pub(crate) struct Style {
    pub ttl: Array3<f32>,
    pub dp: Array3<f32>,
}

/// Load a single voice style JSON into a batch-1 `Style`.
pub(crate) fn load_voice_style<P: AsRef<Path>>(path: P) -> Result<Style, String> {
    let file = File::open(path.as_ref())
        .map_err(|e| format!("open voice style {}: {e}", path.as_ref().display()))?;
    let reader = BufReader::new(file);
    let data: VoiceStyleData =
        serde_json::from_reader(reader).map_err(|e| format!("parse voice style: {e}"))?;

    let ttl_dims = &data.style_ttl.dims;
    let dp_dims = &data.style_dp.dims;

    // Validate dimensions — model JSON must have [batch, dim1, dim2] shape.
    if ttl_dims.len() < 3 {
        return Err(format!(
            "voice style ttl dims too short: expected 3, got {}",
            ttl_dims.len()
        ));
    }
    if dp_dims.len() < 3 {
        return Err(format!(
            "voice style dp dims too short: expected 3, got {}",
            dp_dims.len()
        ));
    }

    // dims = [1, dim1, dim2] — batch dimension is always 1 for a single voice.
    let (ttl_d1, ttl_d2) = (ttl_dims[1], ttl_dims[2]);
    let (dp_d1, dp_d2) = (dp_dims[1], dp_dims[2]);

    let mut ttl_flat = Vec::with_capacity(ttl_d1 * ttl_d2);
    for batch in &data.style_ttl.data {
        for row in batch {
            ttl_flat.extend_from_slice(row);
        }
    }

    let mut dp_flat = Vec::with_capacity(dp_d1 * dp_d2);
    for batch in &data.style_dp.data {
        for row in batch {
            dp_flat.extend_from_slice(row);
        }
    }

    let ttl = Array3::from_shape_vec((1, ttl_d1, ttl_d2), ttl_flat)
        .map_err(|e| format!("reshape ttl style: {e}"))?;
    let dp = Array3::from_shape_vec((1, dp_d1, dp_d2), dp_flat)
        .map_err(|e| format!("reshape dp style: {e}"))?;

    Ok(Style { ttl, dp })
}

// ── Unicode text processor ────────────────────────────────────────────────────

pub(crate) struct UnicodeProcessor {
    indexer: Vec<i64>,
}

impl UnicodeProcessor {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let file =
            File::open(path.as_ref()).map_err(|e| format!("open unicode_indexer.json: {e}"))?;
        let reader = BufReader::new(file);
        let indexer: Vec<i64> =
            serde_json::from_reader(reader).map_err(|e| format!("parse unicode_indexer: {e}"))?;
        Ok(UnicodeProcessor { indexer })
    }

    /// Tokenize a single (text, lang) pair into (token_ids, text_mask).
    pub fn call(
        &self,
        text_list: &[String],
        lang_list: &[String],
    ) -> Result<(Vec<Vec<i64>>, Array3<f32>), String> {
        let mut processed: Vec<String> = Vec::with_capacity(text_list.len());
        for (text, lang) in text_list.iter().zip(lang_list.iter()) {
            processed.push(preprocess_text(text, lang)?);
        }

        let lengths: Vec<usize> = processed.iter().map(|t| t.chars().count()).collect();
        let max_len = *lengths.iter().max().unwrap_or(&0);

        let mut text_ids: Vec<Vec<i64>> = Vec::with_capacity(processed.len());
        for text in &processed {
            let mut row = vec![0i64; max_len];
            for (j, c) in text.chars().enumerate() {
                let val = c as usize;
                row[j] = if val < self.indexer.len() {
                    self.indexer[val]
                } else {
                    -1
                };
            }
            text_ids.push(row);
        }

        let text_mask = get_text_mask(&lengths);
        Ok((text_ids, text_mask))
    }
}

// ── Compiled regex patterns (one-time init) ───────────────────────────────
static RE_EMOJI: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"[\x{1F600}-\x{1F64F}\x{1F300}-\x{1F5FF}\x{1F680}-\x{1F6FF}\x{1F700}-\x{1F77F}\x{1F780}-\x{1F7FF}\x{1F800}-\x{1F8FF}\x{1F900}-\x{1F9FF}\x{1FA00}-\x{1FA6F}\x{1FA70}-\x{1FAFF}\x{2600}-\x{26FF}\x{2700}-\x{27BF}\x{1F1E6}-\x{1F1FF}]+"
    ).unwrap()
});

static RE_SPACE_COMMA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" ,").unwrap());
static RE_SPACE_DOT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" \.").unwrap());
static RE_SPACE_BANG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" !").unwrap());
static RE_SPACE_QUESTION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" \?").unwrap());
static RE_SPACE_SEMI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" ;").unwrap());
static RE_SPACE_COLON: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" :").unwrap());
static RE_SPACE_APOS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" '").unwrap());
static RE_WHITESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static RE_ENDS_PUNC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"[.!?;:,'")\]}…。」』】〉》›»]$"#).unwrap());
static RE_PARAGRAPH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n\s*\n").unwrap());

// ── Text preprocessing ────────────────────────────────────────────────────────

fn preprocess_text(text: &str, lang: &str) -> Result<String, String> {
    let mut s: String = text.nfkd().collect();

    // Strip emojis.
    s = RE_EMOJI.replace_all(&s, "").to_string();

    // Character replacements.
    for (from, to) in &[
        ("–", "-"),
        ("‑", "-"),
        ("—", "-"),
        ("_", " "),
        ("\u{201C}", "\""),
        ("\u{201D}", "\""),
        ("\u{2018}", "'"),
        ("\u{2019}", "'"),
        ("´", "'"),
        ("`", "'"),
        ("[", " "),
        ("]", " "),
        ("|", " "),
        ("/", " "),
        ("#", " "),
        ("→", " "),
        ("←", " "),
    ] {
        s = s.replace(from, to);
    }

    for sym in &["♥", "☆", "♡", "©", "\\"] {
        s = s.replace(sym, "");
    }

    for (from, to) in &[
        ("@", " at "),
        ("e.g.,", "for example, "),
        ("i.e.,", "that is, "),
    ] {
        s = s.replace(from, to);
    }

    // Fix spacing around punctuation.
    s = RE_SPACE_COMMA.replace_all(&s, ",").to_string();
    s = RE_SPACE_DOT.replace_all(&s, ".").to_string();
    s = RE_SPACE_BANG.replace_all(&s, "!").to_string();
    s = RE_SPACE_QUESTION.replace_all(&s, "?").to_string();
    s = RE_SPACE_SEMI.replace_all(&s, ";").to_string();
    s = RE_SPACE_COLON.replace_all(&s, ":").to_string();
    s = RE_SPACE_APOS.replace_all(&s, "'").to_string();

    // Collapse duplicate quote pairs.
    while s.contains("\"\"") {
        s = s.replace("\"\"", "\"");
    }
    while s.contains("''") {
        s = s.replace("''", "'");
    }
    while s.contains("``") {
        s = s.replace("``", "`");
    }

    // Collapse whitespace.
    s = RE_WHITESPACE.replace_all(&s, " ").to_string();
    s = s.trim().to_string();

    // Ensure terminal punctuation.
    if !s.is_empty() && !RE_ENDS_PUNC.is_match(&s) {
        s.push('.');
    }

    if !AVAILABLE_LANGS.contains(&lang) {
        return Err(format!(
            "invalid lang '{lang}'; available: {AVAILABLE_LANGS:?}"
        ));
    }

    Ok(format!("<{lang}>{s}</{lang}>"))
}

// ── Mask / latent helpers ─────────────────────────────────────────────────────

fn length_to_mask(lengths: &[usize], max_len: usize) -> Array3<f32> {
    let bsz = lengths.len();
    let mut mask = Array3::<f32>::zeros((bsz, 1, max_len));
    for (i, &len) in lengths.iter().enumerate() {
        for j in 0..len.min(max_len) {
            mask[[i, 0, j]] = 1.0;
        }
    }
    mask
}

fn get_text_mask(lengths: &[usize]) -> Array3<f32> {
    let max_len = *lengths.iter().max().unwrap_or(&0);
    length_to_mask(lengths, max_len)
}

fn sample_noisy_latent(
    duration: &[f32],
    sample_rate: i32,
    base_chunk_size: i32,
    chunk_compress: i32,
    latent_dim: i32,
) -> (Array3<f32>, Array3<f32>) {
    let bsz = duration.len();
    let max_dur = duration.iter().cloned().fold(0.0f32, f32::max);

    let wav_len_max = (max_dur * sample_rate as f32) as usize;
    let wav_lengths: Vec<usize> = duration
        .iter()
        .map(|&d| (d * sample_rate as f32) as usize)
        .collect();

    let chunk_size = (base_chunk_size * chunk_compress) as usize;
    let latent_len = (wav_len_max + chunk_size - 1) / chunk_size;
    let latent_dim_val = (latent_dim * chunk_compress) as usize;

    let mut noisy = Array3::<f32>::zeros((bsz, latent_dim_val, latent_len));
    let normal = Normal::new(0.0f32, 1.0f32).unwrap();
    let mut rng = rand::thread_rng();

    for b in 0..bsz {
        for d in 0..latent_dim_val {
            for t in 0..latent_len {
                noisy[[b, d, t]] = normal.sample(&mut rng);
            }
        }
    }

    let latent_lengths: Vec<usize> = wav_lengths
        .iter()
        .map(|&len| ((len + chunk_size - 1) / chunk_size).max(1))
        .collect();

    let latent_mask = length_to_mask(&latent_lengths, latent_len);

    // Apply mask.
    for b in 0..bsz {
        for d in 0..latent_dim_val {
            for t in 0..latent_len {
                noisy[[b, d, t]] *= latent_mask[[b, 0, t]];
            }
        }
    }

    (noisy, latent_mask)
}

// ── Text chunking ─────────────────────────────────────────────────────────────

const MAX_CHUNK_LEN: usize = 300;

pub(crate) fn chunk_text(text: &str, max_len: Option<usize>) -> Vec<String> {
    let max_len = max_len.unwrap_or(MAX_CHUNK_LEN);
    let text = text.trim();
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut chunks: Vec<String> = Vec::new();

    for para in RE_PARAGRAPH.split(text) {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }

        if para.len() <= max_len {
            chunks.push(para.to_string());
            continue;
        }

        let sentences = split_sentences(para);
        let mut current = String::new();
        let mut current_len = 0usize;

        for sentence in sentences {
            let sentence = sentence.trim();
            if sentence.is_empty() {
                continue;
            }
            let slen = sentence.len();

            if slen > max_len {
                if !current.is_empty() {
                    chunks.push(current.trim().to_string());
                    current.clear();
                    current_len = 0;
                }
                // Split by comma, then by space.
                for part in sentence.split(',') {
                    let part = part.trim();
                    if part.is_empty() {
                        continue;
                    }
                    let plen = part.len();
                    if plen > max_len {
                        let mut wchunk = String::new();
                        let mut wlen = 0usize;
                        for word in part.split_whitespace() {
                            let wl = word.len();
                            if wlen + wl + 1 > max_len && !wchunk.is_empty() {
                                chunks.push(wchunk.trim().to_string());
                                wchunk.clear();
                                wlen = 0;
                            }
                            if !wchunk.is_empty() {
                                wchunk.push(' ');
                                wlen += 1;
                            }
                            wchunk.push_str(word);
                            wlen += wl;
                        }
                        if !wchunk.is_empty() {
                            chunks.push(wchunk.trim().to_string());
                        }
                    } else {
                        if current_len + plen + 2 > max_len && !current.is_empty() {
                            chunks.push(current.trim().to_string());
                            current.clear();
                            current_len = 0;
                        }
                        if !current.is_empty() {
                            current.push_str(", ");
                            current_len += 2;
                        }
                        current.push_str(part);
                        current_len += plen;
                    }
                }
                continue;
            }

            if current_len + slen + 1 > max_len && !current.is_empty() {
                chunks.push(current.trim().to_string());
                current.clear();
                current_len = 0;
            }
            if !current.is_empty() {
                current.push(' ');
                current_len += 1;
            }
            current.push_str(sentence);
            current_len += slen;
        }

        if !current.is_empty() {
            chunks.push(current.trim().to_string());
        }
    }

    if chunks.is_empty() {
        vec![String::new()]
    } else {
        chunks
    }
}

// ── TextToSpeech ──────────────────────────────────────────────────────────────

pub(crate) struct TextToSpeech {
    cfgs: Config,
    text_processor: UnicodeProcessor,
    dp_ort: Session,
    text_enc_ort: Session,
    vector_est_ort: Session,
    vocoder_ort: Session,
    pub sample_rate: i32,
}

impl TextToSpeech {
    fn new(
        cfgs: Config,
        text_processor: UnicodeProcessor,
        dp_ort: Session,
        text_enc_ort: Session,
        vector_est_ort: Session,
        vocoder_ort: Session,
    ) -> Self {
        let sample_rate = cfgs.ae.sample_rate;
        TextToSpeech {
            cfgs,
            text_processor,
            dp_ort,
            text_enc_ort,
            vector_est_ort,
            vocoder_ort,
            sample_rate,
        }
    }

    fn _infer(
        &mut self,
        text_list: &[String],
        lang_list: &[String],
        style: &Style,
        total_step: usize,
        speed: f32,
    ) -> Result<(Vec<f32>, Vec<f32>), String> {
        let bsz = text_list.len();

        let (text_ids, text_mask) = self.text_processor.call(text_list, lang_list)?;

        let seq_len = text_ids[0].len();
        let flat: Vec<i64> = text_ids.into_iter().flatten().collect();
        let text_ids_arr = Array::from_shape_vec((bsz, seq_len), flat)
            .map_err(|e| format!("reshape text_ids: {e}"))?;

        let text_ids_val =
            Value::from_array(text_ids_arr).map_err(|e| format!("text_ids Value: {e}"))?;
        let text_mask_val =
            Value::from_array(text_mask.clone()).map_err(|e| format!("text_mask Value: {e}"))?;
        let style_dp_val =
            Value::from_array(style.dp.clone()).map_err(|e| format!("style_dp Value: {e}"))?;

        // Duration prediction.
        let dp_out = self
            .dp_ort
            .run(ort::inputs! {
                "text_ids"  => &text_ids_val,
                "style_dp"  => &style_dp_val,
                "text_mask" => &text_mask_val
            })
            .map_err(|e| format!("dp_ort run: {e}"))?;

        let (_, dur_data) = dp_out["duration"]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("extract duration: {e}"))?;
        let mut duration: Vec<f32> = dur_data.to_vec();
        for d in &mut duration {
            *d /= speed;
        }

        // Text encoding.
        let style_ttl_val =
            Value::from_array(style.ttl.clone()).map_err(|e| format!("style_ttl Value: {e}"))?;
        let text_enc_out = self
            .text_enc_ort
            .run(ort::inputs! {
                "text_ids"   => &text_ids_val,
                "style_ttl"  => &style_ttl_val,
                "text_mask"  => &text_mask_val
            })
            .map_err(|e| format!("text_enc_ort run: {e}"))?;

        let (emb_shape, emb_data) = text_enc_out["text_emb"]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("extract text_emb: {e}"))?;
        let text_emb = Array3::from_shape_vec(
            (
                emb_shape[0] as usize,
                emb_shape[1] as usize,
                emb_shape[2] as usize,
            ),
            emb_data.to_vec(),
        )
        .map_err(|e| format!("reshape text_emb: {e}"))?;

        // Noisy latent.
        let (mut xt, latent_mask) = sample_noisy_latent(
            &duration,
            self.sample_rate,
            self.cfgs.ae.base_chunk_size,
            self.cfgs.ttl.chunk_compress_factor,
            self.cfgs.ttl.latent_dim,
        );

        let total_step_arr = Array::from_elem(bsz, total_step as f32);

        // Denoising loop.
        for step in 0..total_step {
            let cur_step_arr = Array::from_elem(bsz, step as f32);

            let xt_val = Value::from_array(xt.clone()).map_err(|e| format!("xt Value: {e}"))?;
            let emb_val =
                Value::from_array(text_emb.clone()).map_err(|e| format!("emb Value: {e}"))?;
            let lmask_val =
                Value::from_array(latent_mask.clone()).map_err(|e| format!("lmask Value: {e}"))?;
            let tmask_val =
                Value::from_array(text_mask.clone()).map_err(|e| format!("tmask Value: {e}"))?;
            let cur_val =
                Value::from_array(cur_step_arr).map_err(|e| format!("cur_step Value: {e}"))?;
            let tot_val = Value::from_array(total_step_arr.clone())
                .map_err(|e| format!("tot_step Value: {e}"))?;
            let sttl_val =
                Value::from_array(style.ttl.clone()).map_err(|e| format!("sttl Value: {e}"))?;

            let ve_out = self
                .vector_est_ort
                .run(ort::inputs! {
                    "noisy_latent" => &xt_val,
                    "text_emb"     => &emb_val,
                    "style_ttl"    => &sttl_val,
                    "latent_mask"  => &lmask_val,
                    "text_mask"    => &tmask_val,
                    "current_step" => &cur_val,
                    "total_step"   => &tot_val
                })
                .map_err(|e| format!("vector_est_ort run step {step}: {e}"))?;

            let (ds, dd) = ve_out["denoised_latent"]
                .try_extract_tensor::<f32>()
                .map_err(|e| format!("extract denoised_latent: {e}"))?;
            xt = Array3::from_shape_vec(
                (ds[0] as usize, ds[1] as usize, ds[2] as usize),
                dd.to_vec(),
            )
            .map_err(|e| format!("reshape denoised_latent: {e}"))?;
        }

        // Vocoder.
        let latent_val = Value::from_array(xt).map_err(|e| format!("final latent Value: {e}"))?;
        let voc_out = self
            .vocoder_ort
            .run(ort::inputs! {
                "latent" => &latent_val
            })
            .map_err(|e| format!("vocoder_ort run: {e}"))?;

        let (_, wav_data) = voc_out["wav_tts"]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("extract wav_tts: {e}"))?;

        Ok((wav_data.to_vec(), duration))
    }

    /// Synthesize `text` in `lang` using `style`.
    ///
    /// Long text is automatically chunked; silence of `silence_secs` seconds
    /// is inserted between chunks. Returns raw f32 PCM at `SAMPLE_RATE`.
    pub(crate) fn call(
        &mut self,
        text: &str,
        lang: &str,
        style: &Style,
        total_step: usize,
        speed: f32,
        silence_secs: f32,
    ) -> Result<Vec<f32>, String> {
        let max_len = if lang == "ko" { 120 } else { 300 };
        let chunks = chunk_text(text, Some(max_len));

        let mut out: Vec<f32> = Vec::new();

        for (i, chunk) in chunks.iter().enumerate() {
            let (wav, duration) = self._infer(
                &[chunk.clone()],
                &[lang.to_string()],
                style,
                total_step,
                speed,
            )?;

            let dur = duration.first().copied().unwrap_or(0.0);
            let wav_len = (self.sample_rate as f32 * dur) as usize;
            let wav_chunk = &wav[..wav_len.min(wav.len())];

            if i > 0 {
                let silence_len = (silence_secs * self.sample_rate as f32) as usize;
                out.extend(std::iter::repeat(0.0f32).take(silence_len));
            }
            out.extend_from_slice(wav_chunk);
        }

        Ok(out)
    }
}

// ── Loader ────────────────────────────────────────────────────────────────────

/// Load all four ONNX sessions and the Unicode tokenizer from `onnx_dir`.
///
/// `onnx_dir` should be `~/.sprout/models/supertonic/`.
pub(crate) fn load_text_to_speech(onnx_dir: &str) -> Result<TextToSpeech, String> {
    let cfgs = load_cfgs(onnx_dir)?;

    let dp_ort = Session::builder()
        .map_err(|e| format!("session builder: {e}"))?
        .commit_from_file(format!("{onnx_dir}/duration_predictor.onnx"))
        .map_err(|e| format!("load duration_predictor: {e}"))?;

    let text_enc_ort = Session::builder()
        .map_err(|e| format!("session builder: {e}"))?
        .commit_from_file(format!("{onnx_dir}/text_encoder.onnx"))
        .map_err(|e| format!("load text_encoder: {e}"))?;

    let vector_est_ort = Session::builder()
        .map_err(|e| format!("session builder: {e}"))?
        .commit_from_file(format!("{onnx_dir}/vector_estimator.onnx"))
        .map_err(|e| format!("load vector_estimator: {e}"))?;

    let vocoder_ort = Session::builder()
        .map_err(|e| format!("session builder: {e}"))?
        .commit_from_file(format!("{onnx_dir}/vocoder.onnx"))
        .map_err(|e| format!("load vocoder: {e}"))?;

    let text_processor = UnicodeProcessor::new(format!("{onnx_dir}/unicode_indexer.json"))?;

    Ok(TextToSpeech::new(
        cfgs,
        text_processor,
        dp_ort,
        text_enc_ort,
        vector_est_ort,
        vocoder_ort,
    ))
}
