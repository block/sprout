//! Kokoro-82M ONNX TTS engine — single-session inference with IPA G2P.
//!
//! Mental model:
//!
//!   load_text_to_speech(model_dir) → KokoroTTS
//!   load_voice_style(path)         → VoiceStyle
//!   tts.call(text, lang, &style)   → Vec<f32> @ 24 kHz
//!
//!   ┌──────────┐   G2P    ┌──────────┐  tokenize  ┌──────────┐
//!   │ raw text │ ──────→  │ IPA str  │ ─────────→ │ int64[]  │
//!   └──────────┘ lexicon  └──────────┘  115-char   └────┬─────┘
//!                                                        │
//!   ┌──────────┐  style   ┌──────────┐  ONNX      ┌────▼─────┐
//!   │ .bin file│ ──────→  │ [1, 256] │ ─────────→ │ Vec<f32> │
//!   └──────────┘ indexed  └──────────┘  session    └──────────┘
//!               by token count                      24 kHz PCM
//!
//! G2P strategy: dictionary lookup (us_gold.json, Apache-2.0 via misaki).
//! OOV words are spelled letter-by-letter using a static IPA table.
//! No espeak dependency — fully GPL-free.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use ndarray::{Array1, Array2};
use ort::{session::Session, value::Value};

use super::preprocessing::split_sentences;

// ── Public constants ──────────────────────────────────────────────────────────

pub const SAMPLE_RATE: u32 = 24_000;
pub const DEFAULT_VOICE: &str = "af_heart";

// Maximum phoneme tokens before padding (model context = 512, minus 2 pad tokens).
const MAX_PHONEME_TOKENS: usize = 510;

// ── VoiceStyle ────────────────────────────────────────────────────────────────

/// Raw f32 voice embedding loaded from a `<name>.bin` file.
///
/// The binary is a flat array of shape `[-1, 256]` in row-major order.
/// Row `i` is the style vector for an utterance with `i` phoneme tokens.
/// This encodes both speaker identity and sequence-length-dependent prosody.
#[derive(Debug)]
pub struct VoiceStyle {
    data: Vec<f32>, // flat: row i = data[i*256 .. (i+1)*256]
}

impl VoiceStyle {
    /// Return the 256-dim style vector for a given phoneme token count.
    /// Clamps to the last available row if `token_count` is out of range.
    fn get(&self, token_count: usize) -> &[f32] {
        let max_rows = self.data.len() / 256;
        let idx = token_count.min(max_rows.saturating_sub(1));
        &self.data[idx * 256..(idx + 1) * 256]
    }
}

/// Load a voice style from a raw little-endian f32 binary file.
pub fn load_voice_style(path: &Path) -> Result<VoiceStyle, String> {
    let bytes = fs::read(path).map_err(|e| format!("read voice {}: {e}", path.display()))?;
    if bytes.len() % 4 != 0 {
        return Err(format!(
            "voice file {} has non-multiple-of-4 byte count ({})",
            path.display(),
            bytes.len()
        ));
    }
    let data: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    if data.len() < 256 {
        return Err(format!(
            "voice file {} too small ({} floats, need at least 256)",
            path.display(),
            data.len()
        ));
    }
    if data.len() % 256 != 0 {
        return Err(format!(
            "voice style has {} floats — expected a multiple of 256 (got {} remainder)",
            data.len(),
            data.len() % 256,
        ));
    }
    Ok(VoiceStyle { data })
}

// ── Tokenizer ─────────────────────────────────────────────────────────────────

/// Static 115-entry IPA char → int64 lookup table.
/// IDs are non-contiguous (0–177); unknown chars are silently dropped.
/// Pad token '$' = 0 is prepended and appended to every sequence.
fn build_vocab() -> HashMap<char, i64> {
    // Source: onnx-community/Kokoro-82M-v1.0-ONNX tokenizer.json
    #[rustfmt::skip]
    let entries: &[(char, i64)] = &[
        ('$', 0),
        (';', 1), (':', 2), (',', 3), ('.', 4), ('!', 5), ('?', 6),
        ('—', 9), ('…', 10), ('"', 11), ('(', 12), (')', 13), ('\u{201c}', 14), ('\u{201d}', 15),
        (' ', 16), ('\u{0303}', 17),
        ('ʣ', 18), ('ʥ', 19), ('ʦ', 20), ('ʨ', 21), ('ᵝ', 22), ('ꭧ', 23),
        ('A', 24), ('I', 25), ('O', 31), ('Q', 33), ('S', 35), ('T', 36),
        ('W', 39), ('Y', 41), ('ᵊ', 42),
        ('a', 43), ('b', 44), ('c', 45), ('d', 46), ('e', 47), ('f', 48),
        ('h', 50), ('i', 51), ('j', 52), ('k', 53), ('l', 54), ('m', 55),
        ('n', 56), ('o', 57), ('p', 58), ('q', 59), ('r', 60), ('s', 61),
        ('t', 62), ('u', 63), ('v', 64), ('w', 65), ('x', 66), ('y', 67), ('z', 68),
        ('ɑ', 69), ('ɐ', 70), ('ɒ', 71), ('æ', 72), ('β', 75), ('ɔ', 76),
        ('ɕ', 77), ('ç', 78), ('ɖ', 80), ('ð', 81), ('ʤ', 82), ('ə', 83),
        ('ɚ', 85), ('ɛ', 86), ('ɜ', 87), ('ɟ', 90), ('ɡ', 92), ('ɥ', 99),
        ('ɨ', 101), ('ɪ', 102), ('ʝ', 103), ('ɯ', 110), ('ɰ', 111),
        ('ŋ', 112), ('ɳ', 113), ('ɲ', 114), ('ɴ', 115), ('ø', 116),
        ('ɸ', 118), ('θ', 119), ('œ', 120), ('ɹ', 123), ('ɾ', 125),
        ('ɻ', 126), ('ʁ', 128), ('ɽ', 129), ('ʂ', 130), ('ʃ', 131),
        ('ʈ', 132), ('ʧ', 133), ('ʊ', 135), ('ʋ', 136), ('ʌ', 138),
        ('ɣ', 139), ('ɤ', 140), ('χ', 142), ('ʎ', 143), ('ʒ', 147),
        ('ʔ', 148), ('ˈ', 156), ('ˌ', 157), ('ː', 158), ('ʰ', 162),
        ('ʲ', 164), ('↓', 169), ('→', 171), ('↗', 172), ('↘', 173), ('ᵻ', 177),
    ];
    entries.iter().copied().collect()
}

/// Convert an IPA phoneme string to a padded int64 token sequence.
/// Returns `[0, id1, id2, ..., idN, 0]` clamped to MAX_PHONEME_TOKENS+2.
/// The pre-pad token count (ids.len() - 2) is used to index the style vector.
fn tokenize(phonemes: &str, vocab: &HashMap<char, i64>) -> Vec<i64> {
    let mut ids: Vec<i64> = vec![0]; // BOS pad
    for id in phonemes
        .chars()
        .filter_map(|c| vocab.get(&c).copied())
        .take(MAX_PHONEME_TOKENS)
    {
        ids.push(id);
    }
    ids.push(0); // EOS pad
    ids
}

// ── G2P Lexicon ───────────────────────────────────────────────────────────────

/// Grapheme-to-phoneme engine with a four-tier fallback chain:
///
///   1. Misaki gold+silver dicts (183K words, Kokoro-native IPA)
///   2. CMUdict (135K words, ARPAbet→Kokoro IPA) — covers inflected forms
///   3. Morphological suffix stripping (-s/-ed/-ing) + retry tiers 1-2
///   4. Letter-by-letter spelling
///
/// All dictionaries are Apache-2.0 or BSD licensed. No GPL.
struct Lexicon {
    /// Misaki gold+silver merged dictionary (Kokoro-native IPA).
    misaki: HashMap<String, String>,
    /// CMU Pronouncing Dictionary (ARPAbet converted to Kokoro IPA at load time).
    cmudict: HashMap<String, String>,
}

/// IPA pronunciations for individual letter names (used for OOV words).
fn letter_ipa(c: char) -> &'static str {
    match c {
        'a' => "ˈeɪ",
        'b' => "bˈiː",
        'c' => "sˈiː",
        'd' => "dˈiː",
        'e' => "ˈiː",
        'f' => "ˈɛf",
        'g' => "dʒˈiː",
        'h' => "ˈeɪtʃ",
        'i' => "ˈaɪ",
        'j' => "dʒˈeɪ",
        'k' => "kˈeɪ",
        'l' => "ˈɛl",
        'm' => "ˈɛm",
        'n' => "ˈɛn",
        'o' => "ˈoʊ",
        'p' => "pˈiː",
        'q' => "kjˈuː",
        'r' => "ˈɑːɹ",
        's' => "ˈɛs",
        't' => "tˈiː",
        'u' => "jˈuː",
        'v' => "vˈiː",
        'w' => "dˈʌbəljˌuː",
        'x' => "ˈɛks",
        'y' => "wˈaɪ",
        'z' => "zˈiː",
        _ => "",
    }
}

/// Punctuation chars that are valid Kokoro vocab tokens and should pass through.
fn is_passthrough_punct(c: char) -> bool {
    matches!(c, ';' | ':' | ',' | '.' | '!' | '?' | '—' | '…' | ' ')
}

/// Vowels that trigger US English /t/→/ɾ/ flapping (misaki's US_TAUS).
const US_TAUS: &str = "AIOWYiuæɑəɛɪɹʊʌ";

/// ARPAbet → Kokoro IPA conversion. Stress digit is stripped before lookup.
fn arpabet_to_ipa(phoneme: &str) -> &'static str {
    match phoneme {
        "AA" => "ɑ",
        "AE" => "æ",
        "AH" => "ʌ",
        "AO" => "ɔ",
        "AW" => "W",
        "AY" => "I",
        "EH" => "ɛ",
        "ER" => "ɜɹ",
        "EY" => "A",
        "IH" => "ɪ",
        "IY" => "i",
        "OW" => "O",
        "OY" => "Y",
        "UH" => "ʊ",
        "UW" => "u",
        "B" => "b",
        "CH" => "ʧ",
        "D" => "d",
        "DH" => "ð",
        "F" => "f",
        "G" => "ɡ",
        "HH" => "h",
        "JH" => "ʤ",
        "K" => "k",
        "L" => "l",
        "M" => "m",
        "N" => "n",
        "NG" => "ŋ",
        "P" => "p",
        "R" => "ɹ",
        "S" => "s",
        "SH" => "ʃ",
        "T" => "t",
        "TH" => "θ",
        "V" => "v",
        "W" => "w",
        "Y" => "j",
        "Z" => "z",
        "ZH" => "ʒ",
        _ => "",
    }
}

/// Convert a CMUdict ARPAbet pronunciation line to Kokoro IPA.
/// Input: "K R IY0 EY1 T AH0 D" → Output: "kɹiˈAtəd"
fn arpabet_line_to_ipa(arpabet: &str) -> String {
    let mut out = String::new();
    for token in arpabet.split_whitespace() {
        // Split phoneme from stress digit (e.g., "EY1" → "EY", Some('1'))
        let (base, stress) = if token.ends_with(|c: char| c.is_ascii_digit()) {
            (&token[..token.len() - 1], token.as_bytes().last().copied())
        } else {
            (token, None)
        };
        // Stress marker goes BEFORE the vowel's IPA
        match stress {
            Some(b'1') => out.push('ˈ'), // primary
            Some(b'2') => out.push('ˌ'), // secondary
            _ => {}
        }
        // AH with stress=0 is schwa (ə), not ʌ
        if base == "AH" && stress == Some(b'0') {
            out.push('ə');
        } else if base == "ER" && stress == Some(b'0') {
            // Unstressed ER is just əɹ
            out.push_str("əɹ");
        } else {
            out.push_str(arpabet_to_ipa(base));
        }
    }
    out
}

impl Lexicon {
    /// Load misaki gold+silver dicts and CMUdict.
    fn load(gold_path: &Path, silver_path: &Path, cmudict_path: &Path) -> Result<Self, String> {
        let mut misaki = Self::load_json(silver_path)?;
        let gold = Self::load_json(gold_path)?;
        misaki.extend(gold);

        let cmudict = if cmudict_path.exists() {
            Self::load_cmudict(cmudict_path)?
        } else {
            eprintln!(
                "sprout-desktop: CMUdict not found at {} — inflected forms may be spelled out",
                cmudict_path.display()
            );
            HashMap::new()
        };

        eprintln!(
            "sprout-desktop: G2P loaded — misaki: {} words, cmudict: {} words",
            misaki.len(),
            cmudict.len()
        );
        Ok(Lexicon { misaki, cmudict })
    }

    fn load_json(path: &Path) -> Result<HashMap<String, String>, String> {
        let content =
            fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let raw: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| format!("parse {}: {e}", path.display()))?;
        let obj = raw
            .as_object()
            .ok_or_else(|| format!("{}: expected JSON object", path.display()))?;
        let mut dict = HashMap::with_capacity(obj.len());
        for (word, val) in obj {
            let ipa = match val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Object(m) => m
                    .get("DEFAULT")
                    .or_else(|| m.values().next())
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                _ => continue,
            };
            if !ipa.is_empty() {
                dict.insert(word.to_lowercase(), ipa);
            }
        }
        Ok(dict)
    }

    /// Load CMUdict and convert ARPAbet → Kokoro IPA at load time.
    /// Format: "WORD PH1 PH2 PH3\n" (single space between word and phonemes).
    /// Variant pronunciations like "WORD(2)" are skipped — we take the first.
    fn load_cmudict(path: &Path) -> Result<HashMap<String, String>, String> {
        let content = fs::read_to_string(path).map_err(|e| format!("read cmudict: {e}"))?;
        let mut dict = HashMap::with_capacity(140_000);
        for line in content.lines() {
            // Skip comments and blank lines
            if line.starts_with(";;;") || line.is_empty() {
                continue;
            }
            // Split on first space
            let (word, phonemes) = match line.find(' ') {
                Some(i) => (&line[..i], line[i + 1..].trim()),
                None => continue,
            };
            // Skip variant pronunciations like "WORD(2)"
            if word.contains('(') {
                continue;
            }
            let key = word.to_lowercase();
            let ipa = arpabet_line_to_ipa(phonemes);
            if !ipa.is_empty() {
                dict.entry(key).or_insert(ipa);
            }
        }
        Ok(dict)
    }

    /// Look up a word across all tiers. Returns None if not found anywhere.
    fn lookup(&self, word: &str) -> Option<String> {
        self.misaki
            .get(word)
            .cloned()
            .or_else(|| self.cmudict.get(word).cloned())
    }

    /// Apply English -s/-es/-ies suffix phoneme rules (misaki's `_s`).
    fn apply_s(stem_ipa: &str) -> String {
        let last = stem_ipa.chars().last().unwrap_or(' ');
        if "ptkfθ".contains(last) {
            format!("{stem_ipa}s")
        } else if "szʃʒʧʤ".contains(last) {
            format!("{stem_ipa}ᵻz")
        } else {
            format!("{stem_ipa}z")
        }
    }

    /// Apply English -ed suffix phoneme rules (misaki's `_ed`).
    fn apply_ed(stem_ipa: &str) -> String {
        let chars: Vec<char> = stem_ipa.chars().collect();
        let last = *chars.last().unwrap_or(&' ');
        if "pkfθʃsʧ".contains(last) {
            format!("{stem_ipa}t")
        } else if last == 'd' {
            format!("{stem_ipa}ᵻd")
        } else if last != 't' {
            format!("{stem_ipa}d")
        } else if chars.len() >= 2 && US_TAUS.contains(chars[chars.len() - 2]) {
            // US flap: "created" → kɹiˈAɾᵻd
            let mut out: String = chars[..chars.len() - 1].iter().collect();
            out.push_str("ɾᵻd");
            out
        } else {
            format!("{stem_ipa}ᵻd")
        }
    }

    /// Apply English -ing suffix phoneme rules (misaki's `_ing`).
    fn apply_ing(stem_ipa: &str) -> String {
        let chars: Vec<char> = stem_ipa.chars().collect();
        let last = *chars.last().unwrap_or(&' ');
        if last == 't' && chars.len() >= 2 && US_TAUS.contains(chars[chars.len() - 2]) {
            // US flap: "creating" → kɹiˈAɾɪŋ
            let mut out: String = chars[..chars.len() - 1].iter().collect();
            out.push_str("ɾɪŋ");
            out
        } else {
            format!("{stem_ipa}ɪŋ")
        }
    }

    /// Try stripping -s/-ed/-ing suffix, look up the base, and re-apply phonetically.
    fn try_morphological(&self, word: &str) -> Option<String> {
        // Try -s / -es / -ies
        if word.len() >= 3 && word.ends_with('s') {
            // -ies → base + y
            if word.len() > 4 && word.ends_with("ies") {
                if let Some(stem) = self.lookup(&format!("{}y", &word[..word.len() - 3])) {
                    return Some(Self::apply_s(&stem));
                }
            }
            // -es → base
            if word.len() > 4 && word.ends_with("es") && !word.ends_with("ies") {
                if let Some(stem) = self.lookup(&word[..word.len() - 2]) {
                    return Some(Self::apply_s(&stem));
                }
            }
            // -s → base
            if !word.ends_with("ss") {
                if let Some(stem) = self.lookup(&word[..word.len() - 1]) {
                    return Some(Self::apply_s(&stem));
                }
            }
        }
        // Try -ed / -d
        if word.len() >= 4 && word.ends_with('d') {
            // -ed → base (not -eed)
            if word.len() > 4 && word.ends_with("ed") && !word.ends_with("eed") {
                if let Some(stem) = self.lookup(&word[..word.len() - 2]) {
                    return Some(Self::apply_ed(&stem));
                }
                // -ed where base ends in e: "created" → "create"
                if let Some(stem) = self.lookup(&format!("{}e", &word[..word.len() - 2])) {
                    return Some(Self::apply_ed(&stem));
                }
            }
            // -d → base (e.g., "discovered" → strip "d" → "discovere" fails,
            // but "configured" → strip "d" → "configure" works)
            if !word.ends_with("dd") {
                if let Some(stem) = self.lookup(&word[..word.len() - 1]) {
                    return Some(Self::apply_ed(&stem));
                }
            }
        }
        // Try -ing
        if word.len() >= 5 && word.ends_with("ing") {
            let base = &word[..word.len() - 3];
            // -ing → base (e.g., "running" base = "runn" — won't match, need double-consonant)
            if let Some(stem) = self.lookup(base) {
                return Some(Self::apply_ing(&stem));
            }
            // -ing + e → base+e (e.g., "creating" → "creat" + "e" = "create")
            if let Some(stem) = self.lookup(&format!("{base}e")) {
                return Some(Self::apply_ing(&stem));
            }
            // Double consonant: "running" → "run"
            if base.len() >= 2 {
                let bytes = base.as_bytes();
                if bytes[bytes.len() - 1] == bytes[bytes.len() - 2] {
                    if let Some(stem) = self.lookup(&base[..base.len() - 1]) {
                        return Some(Self::apply_ing(&stem));
                    }
                }
            }
        }
        None
    }

    /// Convert a single word to IPA using the full fallback chain.
    fn word_to_ipa(&self, word: &str) -> String {
        // Compound words: split on hyphens and underscores, process each part
        // independently. "short-and-natural" → "short" + "and" + "natural",
        // "parent_event_id" → "parent" + "event" + "id".
        // Each part gets full dict lookup. Joined with a space (brief TTS pause).
        if word.contains('-') || word.contains('_') {
            let parts: Vec<String> = word
                .split(|c: char| c == '-' || c == '_')
                .filter(|p| !p.is_empty())
                .map(|p| self.word_to_ipa(p))
                .collect();
            return parts.join(" ");
        }

        // Normalize curly quotes to straight apostrophes.
        let normalized = word.replace('\u{2019}', "'").replace('\u{2018}', "'");
        let stripped: String = normalized
            .chars()
            .filter(|c| c.is_alphabetic() || *c == '\'')
            .collect::<String>()
            .to_lowercase();

        // Tier 1+2: misaki + CMUdict direct lookup
        if let Some(ipa) = self.lookup(&stripped) {
            return ipa;
        }

        // Contractions: "don't" → "don" + "'t"
        if let Some(apos_idx) = stripped.find('\'') {
            let base = &stripped[..apos_idx];
            let suffix = &stripped[apos_idx..];
            if let Some(base_ipa) = self.lookup(base) {
                let suffix_ipa = self.lookup(suffix).unwrap_or_else(|| match suffix {
                    "'ve" => "v".to_string(),
                    "'re" => "ɹ".to_string(),
                    _ => String::new(),
                });
                if !suffix_ipa.is_empty() {
                    return format!("{base_ipa}{suffix_ipa}");
                }
            }
        }

        // Tier 3: morphological suffix stripping
        if let Some(ipa) = self.try_morphological(&stripped) {
            return ipa;
        }

        // Tier 4: letter-by-letter spelling
        stripped
            .chars()
            .filter(|c| c.is_alphabetic())
            .map(letter_ipa)
            .collect()
    }

    /// Convert a full text chunk to an IPA phoneme string.
    fn text_to_ipa(&self, text: &str) -> String {
        let mut out = String::new();
        for token in text.split_whitespace() {
            if !out.is_empty() {
                out.push(' ');
            }
            let leading: String = token
                .chars()
                .take_while(|c| is_passthrough_punct(*c))
                .collect();
            let trailing: String = token
                .chars()
                .rev()
                .take_while(|c| is_passthrough_punct(*c))
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            let word = &token[leading.len()..token.len() - trailing.len()];
            out.push_str(&leading);
            if !word.is_empty() {
                out.push_str(&self.word_to_ipa(word));
            }
            out.push_str(&trailing);
        }
        out
    }
}

// ── KokoroTTS ─────────────────────────────────────────────────────────────────

pub struct KokoroTTS {
    session: Session,
    vocab: HashMap<char, i64>,
    lexicon: Lexicon,
    // Retained for potential future use (e.g., hot-reloading voices by path).
    #[allow(dead_code)]
    model_dir: PathBuf,
}

/// Load the Kokoro TTS engine from a model directory.
///
/// Expects:
///   `<model_dir>/model.onnx`  (or model_quantized.onnx — tries both)
///   `<model_dir>/us_gold.json` (G2P dictionary)
///
/// CoreML execution provider is registered with auto-fallback to CPU.
/// The compiled CoreML model is cached in `<model_dir>/.coreml_cache/`.
pub fn load_text_to_speech(model_dir: &str) -> Result<KokoroTTS, String> {
    let model_dir_path = PathBuf::from(model_dir);

    // Try quantized model first for speed, fall back to full-precision.
    let model_path = ["model_quantized.onnx", "model_q8f16.onnx", "model.onnx"]
        .iter()
        .map(|name| model_dir_path.join(name))
        .find(|p| p.exists())
        .ok_or_else(|| format!("no model.onnx found in {model_dir}"))?;

    // Try CoreML first (zero binary cost — macOS system framework).
    // If the model has ops CoreML can't handle (common with quantized models),
    // the EP registers fine but commit_from_file fails. Catch that and retry
    // with CPU-only. This is the expected path for model_q8f16.onnx.
    let session = {
        let mut builder_with_coreml = Session::builder()
            .map_err(|e| format!("session builder: {e}"))?
            .with_execution_providers([ort::ep::CoreML::default()
                .with_compute_units(ort::ep::coreml::ComputeUnits::All)
                .with_model_format(ort::ep::coreml::ModelFormat::MLProgram)
                .with_model_cache_dir(model_dir_path.join(".coreml_cache").to_string_lossy())
                .build()])
            .map_err(|e| format!("execution provider: {e}"))?;

        match builder_with_coreml.commit_from_file(&model_path) {
            Ok(s) => {
                eprintln!("sprout-desktop: Kokoro loaded with CoreML acceleration");
                s
            }
            Err(coreml_err) => {
                eprintln!(
                    "sprout-desktop: CoreML failed for {}, falling back to CPU: {coreml_err}",
                    model_path.display()
                );
                // Retry without any execution providers — pure CPU.
                Session::builder()
                    .map_err(|e| format!("session builder (CPU fallback): {e}"))?
                    .commit_from_file(&model_path)
                    .map_err(|e| format!("load model {} (CPU): {e}", model_path.display()))?
            }
        }
    };

    let gold_path = model_dir_path.join("us_gold.json");
    let silver_path = model_dir_path.join("us_silver.json");
    let cmudict_path = model_dir_path.join("cmudict.dict");
    let lexicon = Lexicon::load(&gold_path, &silver_path, &cmudict_path)?;

    Ok(KokoroTTS {
        session,
        vocab: build_vocab(),
        lexicon,
        model_dir: model_dir_path,
    })
}

impl KokoroTTS {
    /// Synthesize `text` to 24 kHz mono PCM.
    ///
    /// - `_total_step` is ignored — Kokoro is not diffusion-based.
    /// - `speed` controls speech rate (0.5–2.0; 1.0 = normal).
    /// - `silence_secs` of silence is inserted between sentence chunks.
    /// - `lang` is accepted for API compatibility but currently unused
    ///   (Kokoro v1.0 language is selected by voice name prefix, e.g. `af_*`).
    pub fn call(
        &mut self,
        text: &str,
        _lang: &str,
        style: &VoiceStyle,
        _total_step: usize,
        speed: f32,
        silence_secs: f32,
    ) -> Result<Vec<f32>, String> {
        let silence_samples = (silence_secs * SAMPLE_RATE as f32) as usize;
        let silence = vec![0.0f32; silence_samples];

        let sentences = split_sentences(text);
        let mut output: Vec<f32> = Vec::new();

        for (i, sentence) in sentences.iter().enumerate() {
            let chunk_audio = self.synth_chunk(sentence, _lang, style, _total_step, speed)?;

            if i > 0 && !output.is_empty() {
                output.extend_from_slice(&silence);
            }
            output.extend(chunk_audio);
        }

        Ok(output)
    }

    /// Synthesize a single pre-split text chunk. Caller is responsible for sentence splitting.
    /// This avoids double-splitting when the TTS pipeline has already split the text.
    ///
    /// - `_lang` is accepted for API compatibility but currently unused
    ///   (Kokoro v1.0 language is selected by voice name prefix, e.g. `af_*`).
    /// - `_steps` is accepted for API compatibility but currently unused
    ///   (Kokoro is not diffusion-based).
    pub fn synth_chunk(
        &mut self,
        text: &str,
        _lang: &str,
        style: &VoiceStyle,
        _steps: usize,
        speed: f32,
    ) -> Result<Vec<f32>, String> {
        // G2P: text → IPA phoneme string
        let ipa = self.lexicon.text_to_ipa(text);

        // Tokenize: IPA → int64 ids with BOS/EOS pad tokens
        let token_ids = tokenize(&ipa, &self.vocab);

        // Style vector is indexed by phoneme count (excluding the 2 pad tokens).
        // Shape expected by model: [1, 256] (kokoro-js uses [1, 256], not [1, 1, 256]).
        let phoneme_count = token_ids.len() - 2;
        let style_slice = style.get(phoneme_count);

        // Build ONNX input tensors.
        let seq_len = token_ids.len();
        let input_ids_arr = Array2::from_shape_vec((1, seq_len), token_ids)
            .map_err(|e| format!("input_ids shape: {e}"))?;
        let input_ids_val =
            Value::from_array(input_ids_arr).map_err(|e| format!("input_ids Value: {e}"))?;

        // Style: [1, 256]. Research notes [1, 1, 256] but kokoro-js uses [1, 256].
        let style_arr = Array2::from_shape_vec((1, 256), style_slice.to_vec())
            .map_err(|e| format!("style shape: {e}"))?;
        let style_val = Value::from_array(style_arr).map_err(|e| format!("style Value: {e}"))?;

        let speed_arr = Array1::from_vec(vec![speed]);
        let speed_val = Value::from_array(speed_arr).map_err(|e| format!("speed Value: {e}"))?;

        // Run inference. Output[0] = waveform float32[1, N_samples].
        let outputs = self
            .session
            .run(ort::inputs! {
                "input_ids" => &input_ids_val,
                "style"     => &style_val,
                "speed"     => &speed_val,
            })
            .map_err(|e| format!("onnx run: {e}"))?;

        let (_, waveform) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("extract waveform: {e}"))?;

        Ok(waveform.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Tokenizer ─────────────────────────────────────────────────────────

    #[test]
    fn tokenize_empty_produces_bos_eos() {
        let vocab = build_vocab();
        let ids = tokenize("", &vocab);
        assert_eq!(ids, vec![0, 0]); // BOS + EOS only
    }

    #[test]
    fn tokenize_known_chars() {
        let vocab = build_vocab();
        let ids = tokenize("a", &vocab);
        // 'a' maps to 43 in the vocab
        assert_eq!(ids, vec![0, 43, 0]);
    }

    #[test]
    fn tokenize_unknown_chars_dropped() {
        let vocab = build_vocab();
        let ids = tokenize("🎉", &vocab);
        // Emoji not in vocab — should be dropped, leaving only BOS+EOS
        assert_eq!(ids, vec![0, 0]);
    }

    #[test]
    fn tokenize_respects_max_length() {
        let vocab = build_vocab();
        let long_input: String = "a".repeat(600); // exceeds MAX_PHONEME_TOKENS (510)
        let ids = tokenize(&long_input, &vocab);
        // Should be clamped: BOS + 510 tokens + EOS = 512
        assert_eq!(ids.len(), 512);
        assert_eq!(ids[0], 0); // BOS
        assert_eq!(*ids.last().unwrap(), 0); // EOS
    }

    // ── ARPAbet conversion ────────────────────────────────────────────────

    #[test]
    fn arpabet_simple_word() {
        // "HH AH0 L OW1" = hello
        let ipa = arpabet_line_to_ipa("HH AH0 L OW1");
        assert_eq!(ipa, "həlˈO");
    }

    #[test]
    fn arpabet_stress_markers() {
        // Primary stress before vowel, secondary stress before vowel
        let ipa = arpabet_line_to_ipa("K R IY0 EY1 T");
        // IY0 = unstressed 'i', EY1 = primary 'A'
        assert!(ipa.contains('ˈ'), "should contain primary stress: {ipa}");
    }

    #[test]
    fn arpabet_schwa() {
        // AH0 should produce schwa (ə), not ʌ
        let ipa = arpabet_line_to_ipa("AH0");
        assert_eq!(ipa, "ə");
    }

    #[test]
    fn arpabet_unstressed_er() {
        // ER0 should produce əɹ
        let ipa = arpabet_line_to_ipa("ER0");
        assert_eq!(ipa, "əɹ");
    }

    // ── Letter IPA ────────────────────────────────────────────────────────

    #[test]
    fn letter_ipa_covers_alphabet() {
        for c in 'a'..='z' {
            let ipa = letter_ipa(c);
            assert!(!ipa.is_empty(), "letter_ipa('{c}') returned empty");
        }
    }

    #[test]
    fn letter_ipa_non_alpha_empty() {
        assert_eq!(letter_ipa('1'), "");
        assert_eq!(letter_ipa('!'), "");
    }

    // ── Punctuation passthrough ───────────────────────────────────────────

    #[test]
    fn passthrough_punct_includes_expected() {
        assert!(is_passthrough_punct('.'));
        assert!(is_passthrough_punct('!'));
        assert!(is_passthrough_punct('?'));
        assert!(is_passthrough_punct(' '));
        assert!(is_passthrough_punct(','));
    }

    #[test]
    fn passthrough_punct_excludes_alpha() {
        assert!(!is_passthrough_punct('a'));
        assert!(!is_passthrough_punct('Z'));
    }

    // ── VoiceStyle ────────────────────────────────────────────────────────

    #[test]
    fn voice_style_get_clamps_to_last_row() {
        // 2 rows of 256 floats
        let data: Vec<f32> = (0..512).map(|i| i as f32).collect();
        let style = VoiceStyle { data };
        // Row 0
        assert_eq!(style.get(0)[0], 0.0);
        // Row 1
        assert_eq!(style.get(1)[0], 256.0);
        // Row 999 should clamp to row 1 (last available)
        assert_eq!(style.get(999)[0], 256.0);
    }

    #[test]
    fn load_voice_style_rejects_too_small() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("kokoro_test_small");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("tiny.bin");
        let mut f = std::fs::File::create(&path).unwrap();
        // Write only 100 floats (need at least 256)
        for i in 0..100u32 {
            f.write_all(&(i as f32).to_le_bytes()).unwrap();
        }
        drop(f);
        let result = load_voice_style(&path);
        assert!(result.is_err(), "should reject file with < 256 floats");
        assert!(result.unwrap_err().contains("too small"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_voice_style_rejects_non_multiple_of_256() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("kokoro_test_nonaligned");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("nonaligned.bin");
        let mut f = std::fs::File::create(&path).unwrap();
        // Write 257 floats — not a multiple of 256 (remainder = 1)
        for i in 0..257u32 {
            f.write_all(&(i as f32).to_le_bytes()).unwrap();
        }
        drop(f);
        let result = load_voice_style(&path);
        assert!(
            result.is_err(),
            "should reject file with non-multiple-of-256 floats"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("257"),
            "error should mention float count: {err}"
        );
        assert!(
            err.contains("remainder"),
            "error should mention remainder: {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Suffix rules ──────────────────────────────────────────────────────

    #[test]
    fn apply_s_voiceless() {
        // After voiceless consonants: +s
        assert!(Lexicon::apply_s("kæt").ends_with('s'));
    }

    #[test]
    fn apply_s_sibilant() {
        // After sibilants: +ᵻz
        assert!(Lexicon::apply_s("bʌz").ends_with("ᵻz"));
    }

    #[test]
    fn apply_s_voiced() {
        // After voiced consonants: +z
        assert!(Lexicon::apply_s("dɔɡ").ends_with('z'));
    }

    #[test]
    fn apply_ed_voiceless() {
        // After voiceless: +t
        assert!(Lexicon::apply_ed("wɔk").ends_with('t'));
    }

    #[test]
    fn apply_ed_d_ending() {
        // After d: +ᵻd
        assert!(Lexicon::apply_ed("æd").ends_with("ᵻd"));
    }

    #[test]
    fn apply_ing_basic() {
        assert!(Lexicon::apply_ing("rʌn").ends_with("ɪŋ"));
    }

    #[test]
    fn hyphenated_word_splits_into_parts() {
        let lex = Lexicon {
            misaki: HashMap::new(),
            cmudict: HashMap::new(),
        };
        let result = lex.word_to_ipa("short-and-natural");
        let space_count = result.matches(' ').count();
        assert_eq!(
            space_count, 2,
            "expected 2 spaces for 3 hyphenated parts, got {space_count}: {result}"
        );
    }

    #[test]
    fn underscored_word_splits_into_parts() {
        let lex = Lexicon {
            misaki: HashMap::new(),
            cmudict: HashMap::new(),
        };
        let result = lex.word_to_ipa("parent_event_id");
        let space_count = result.matches(' ').count();
        assert_eq!(
            space_count, 2,
            "expected 2 spaces for 3 underscored parts, got {space_count}: {result}"
        );
    }

    #[test]
    fn compound_word_with_dict_lookup() {
        let mut dict = HashMap::new();
        dict.insert("parent".to_string(), "pɛɹənt".to_string());
        dict.insert("event".to_string(), "ɪvɛnt".to_string());
        dict.insert("id".to_string(), "aɪdiː".to_string());
        let lex = Lexicon {
            misaki: dict,
            cmudict: HashMap::new(),
        };
        // Underscore compound
        let result = lex.word_to_ipa("parent_event_id");
        assert!(result.contains("pɛɹənt"), "parent not resolved: {result}");
        assert!(result.contains("ɪvɛnt"), "event not resolved: {result}");
        assert!(result.contains("aɪdiː"), "id not resolved: {result}");

        // Hyphen compound
        let result = lex.word_to_ipa("short-and-sweet");
        assert_eq!(result.matches(' ').count(), 2, "hyphen split: {result}");
    }
}
