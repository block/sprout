//! Text-to-Speech pipeline for huddle agent voice output.
//!
//! Mental model:
//!
//! ```text
//! caller: pipeline.speak("Hello world. How are you?")
//!   → bounded sync_channel (TEXT_QUEUE_DEPTH = 8)
//!   → tts_worker thread (owns 1 Supertonic engine)
//!       1. Preprocess text
//!       2. Split into sentences
//!       3. Synthesize sentence 0 → f32 PCM at 24 kHz
//!       4. Hand samples to rodio (plays on audio thread = parallel)
//!       5. While sentence 0 plays, synthesize sentence 1 on this thread
//!       6. When sentence 0 finishes, immediately play sentence 1 (already ready)
//!       ... (lookahead: synthesis and playback overlap)
//!   → tts_active = true while playing, false when idle
//!   → cancel flag: drain queue + stop playback
//! ```
//!
//! Supertonic synthesis is ~167× faster than real-time, so the synthesis
//! thread keeps well ahead of the audio thread with a single engine.
//!
//! `tts_active` is an `Arc<AtomicBool>` shared with the STT pipeline so STT
//! can gate microphone input while the agent is speaking.

use std::{
    num::NonZero,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
        Arc,
    },
    thread,
    time::Duration,
};

use super::preprocessing::preprocess_for_tts;
use super::supertonic::{self, load_text_to_speech, load_voice_style, Style, TextToSpeech, SAMPLE_RATE};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum number of queued text items.
/// Prevents unbounded accumulation when the agent produces text faster than
/// TTS can play it. Excess items are dropped with a warning.
const TEXT_QUEUE_DEPTH: usize = 8;

/// How long the worker waits on the text channel before checking the shutdown flag.
const RECV_TIMEOUT: Duration = Duration::from_millis(100);

/// Supertonic denoising steps. 5 = good quality/speed tradeoff.
/// Lower (2) = fastest; higher (10) = best quality.
const SYNTH_STEPS: usize = 5;

/// Synthesis speed multiplier. Slightly faster than natural speech.
const SYNTH_SPEED: f32 = 1.05;

// ── Public pipeline handle ────────────────────────────────────────────────────

/// Handle to the running TTS pipeline.
///
/// Not Clone — wrap in `Arc` to share across threads.
#[derive(Debug)]
pub struct TtsPipeline {
    /// Send preprocessed text into the pipeline.
    text_tx: SyncSender<String>,
    /// `true` while the agent is speaking. Shared with the STT pipeline for gating.
    #[allow(dead_code)]
    pub tts_active: Arc<AtomicBool>,
    /// Signals the worker thread to stop.
    shutdown: Arc<AtomicBool>,
    /// Cancel flag: worker drains the queue and stops current playback.
    /// Public so the STT pipeline can share it for barge-in detection.
    pub cancel: Arc<AtomicBool>,
    /// Voice name (e.g. "F1"). Stored for future voice-switching support.
    #[allow(dead_code)]
    voice: String,
    /// Worker thread handle — taken on drop to join cleanly.
    thread: Option<thread::JoinHandle<()>>,
}

impl TtsPipeline {
    /// Spawn the TTS pipeline thread using the default voice.
    ///
    /// `model_dir` must contain the Supertonic model files:
    ///   `duration_predictor.onnx`, `text_encoder.onnx`,
    ///   `vector_estimator.onnx`, `vocoder.onnx`,
    ///   `unicode_indexer.json`, `tts.json`, `<voice>.json`
    ///
    /// `tts_active` is set to `true` while audio is playing and `false` when idle.
    /// Pass the same `Arc` to the STT pipeline to gate microphone input.
    pub fn new(model_dir: PathBuf, tts_active: Arc<AtomicBool>) -> Result<Self, String> {
        Self::new_with_voice(model_dir, tts_active, supertonic::DEFAULT_VOICE)
    }

    /// Spawn the TTS pipeline thread with a specific voice name (e.g. `"F1"`, `"M3"`).
    pub fn new_with_voice(
        model_dir: PathBuf,
        tts_active: Arc<AtomicBool>,
        voice: &str,
    ) -> Result<Self, String> {
        let (text_tx, text_rx) = mpsc::sync_channel::<String>(TEXT_QUEUE_DEPTH);
        let shutdown = Arc::new(AtomicBool::new(false));
        let cancel = Arc::new(AtomicBool::new(false));

        let shutdown_worker = Arc::clone(&shutdown);
        let cancel_worker = Arc::clone(&cancel);
        let tts_active_worker = Arc::clone(&tts_active);
        let voice_name = voice.to_string();
        let model_dir_worker = model_dir.clone();

        let handle = thread::Builder::new()
            .name("tts-worker".into())
            .spawn(move || {
                tts_worker(
                    model_dir_worker,
                    voice_name,
                    text_rx,
                    tts_active_worker,
                    shutdown_worker,
                    cancel_worker,
                )
            })
            .map_err(|e| format!("failed to spawn tts-worker thread: {e}"))?;

        Ok(Self {
            text_tx,
            tts_active,
            shutdown,
            cancel,
            voice: voice.to_string(),
            thread: Some(handle),
        })
    }

    /// Queue `text` for TTS synthesis and playback.
    ///
    /// Non-blocking. Returns `Err` if the queue is full (bounded at
    /// `TEXT_QUEUE_DEPTH`) — caller may log and discard.
    pub fn speak(&self, text: String) -> Result<(), String> {
        self.text_tx
            .try_send(text)
            .map_err(|e| format!("TTS queue full, dropping: {e}"))
    }

    /// Barge-in: cancel current speech and discard queued items.
    ///
    /// Sets the cancel flag. The worker will drain the queue and stop the
    /// current rodio Player on its next iteration.
    #[allow(dead_code)]
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    /// Signal the worker thread to stop.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
    }
}

impl Drop for TtsPipeline {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        // Dropping `text_tx` unblocks the worker's recv_timeout loop.
        // Join to ensure the audio thread exits cleanly.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

// ── Worker thread ─────────────────────────────────────────────────────────────

fn tts_worker(
    model_dir: PathBuf,
    voice_name: String,
    text_rx: mpsc::Receiver<String>,
    tts_active: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
) {
    // ── 1. Initialise Supertonic engine ───────────────────────────────────────
    let model_dir_str = model_dir.to_string_lossy().to_string();

    let mut engine = match load_text_to_speech(&model_dir_str) {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "sprout-desktop: TTS Supertonic init failed (model_dir={}): {e}. TTS disabled.",
                model_dir.display()
            );
            drain_text_channel(text_rx, &shutdown);
            return;
        }
    };

    // ── 2. Load voice style ───────────────────────────────────────────────────
    let voice_path = model_dir.join(format!("{voice_name}.json"));
    let style = match load_voice_style(&voice_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "sprout-desktop: TTS voice style load failed ({voice_name}): {e}. TTS disabled."
            );
            drain_text_channel(text_rx, &shutdown);
            return;
        }
    };

    // ── 3. Initialise rodio output device ─────────────────────────────────────
    use rodio::{DeviceSinkBuilder, Player};

    let sink_handle = match DeviceSinkBuilder::open_default_sink() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("sprout-desktop: TTS audio output failed: {e}. TTS disabled.");
            drain_text_channel(text_rx, &shutdown);
            return;
        }
    };

    // ── 4. Main loop ──────────────────────────────────────────────────────────
    loop {
        if shutdown.load(Ordering::Acquire) {
            break;
        }

        // Handle cancel: drain queue and clear the flag.
        if cancel.load(Ordering::Acquire) {
            while text_rx.try_recv().is_ok() {}
            cancel.store(false, Ordering::Release);
            tts_active.store(false, Ordering::Release);
            continue;
        }

        let raw_text = match text_rx.recv_timeout(RECV_TIMEOUT) {
            Ok(t) => t,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        // Check cancel again after unblocking — a cancel may have arrived
        // while we were waiting.
        if cancel.load(Ordering::Acquire) {
            while text_rx.try_recv().is_ok() {}
            cancel.store(false, Ordering::Release);
            continue;
        }

        // Preprocess text.
        let text = preprocess_for_tts(&raw_text);
        if text.is_empty() {
            continue;
        }

        // Split into sentences and synth with lookahead — pre-synthesize
        // sentence N+1 while sentence N plays on the rodio audio thread,
        // eliminating inter-sentence gaps.
        let sentences: Vec<String> = split_sentences(&text)
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();

        if sentences.is_empty() {
            continue;
        }

        use rodio::buffer::SamplesBuffer;
        let channels = NonZero::new(1u16).expect("1 is nonzero");
        let rate = NonZero::new(SAMPLE_RATE).expect("24000 is nonzero");

        tts_active.store(true, Ordering::Release);

        // Eagerly synthesize the first sentence before entering the loop.
        let mut pending_audio: Option<Vec<f32>> =
            synth_sentence(&mut engine, &sentences[0], &style);

        for i in 0..sentences.len() {
            if cancel.load(Ordering::Acquire) || shutdown.load(Ordering::Acquire) {
                break;
            }

            // Take the already-synthesized audio for this sentence.
            let current_audio = pending_audio.take();
            let next_idx = i + 1;

            if let Some(samples) = current_audio {
                // Hand samples to rodio — playback starts immediately on the
                // audio thread while this thread continues below.
                let buf = SamplesBuffer::new(channels, rate, samples);
                let player = Player::connect_new(&sink_handle.mixer());
                player.append(buf);

                // While this sentence plays, synthesize the next one.
                // Rodio plays on its own audio thread, so synthesis here is
                // truly parallel with playback.
                if next_idx < sentences.len() {
                    pending_audio =
                        synth_sentence(&mut engine, &sentences[next_idx], &style);
                }

                // Wait for current playback to finish.
                loop {
                    if cancel.load(Ordering::Acquire) || shutdown.load(Ordering::Acquire) {
                        player.clear();
                        while text_rx.try_recv().is_ok() {}
                        cancel.store(false, Ordering::Release);
                        break;
                    }
                    if player.empty() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            } else if next_idx < sentences.len() {
                // No audio for this sentence (empty/error) — still synthesize next.
                pending_audio = synth_sentence(&mut engine, &sentences[next_idx], &style);
            }
        }

        tts_active.store(false, Ordering::Release);

        if shutdown.load(Ordering::Acquire) {
            break;
        }
    }

    tts_active.store(false, Ordering::Release);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Synthesize a single sentence, returning f32 PCM samples or `None` on error/empty.
fn synth_sentence(engine: &mut TextToSpeech, text: &str, style: &Style) -> Option<Vec<f32>> {
    match engine.call(text, "en", style, SYNTH_STEPS, SYNTH_SPEED, 0.0) {
        Ok(samples) if !samples.is_empty() => Some(samples),
        Ok(_) => None,
        Err(e) => {
            eprintln!("sprout-desktop: TTS synth failed for {text:?}: {e}");
            None
        }
    }
}

/// Split text into sentence-sized chunks for TTS.
///
/// Splits on `.` `!` `?` followed by whitespace, plus `\n` and `—`.
/// Keeps chunks non-empty and trimmed.
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let c = chars[i];
        current.push(c);

        let is_break = match c {
            '.' | '!' | '?' => {
                // Only break if followed by whitespace or end of text,
                // AND preceded by a letter (not a digit — avoids splitting "1." "2." etc.)
                let prev_is_letter = i > 0 && chars[i - 1].is_alphabetic();
                let next_is_boundary = i + 1 >= len || chars[i + 1].is_whitespace();
                prev_is_letter && next_is_boundary
            }
            '\n' => true,
            '—' => true,
            _ => false,
        };

        if is_break {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                sentences.push(trimmed);
            }
            current.clear();
        }

        i += 1;
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        sentences.push(trimmed);
    }

    sentences
}

/// Drain and discard all pending text until shutdown or disconnect.
fn drain_text_channel(rx: mpsc::Receiver<String>, shutdown: &AtomicBool) {
    loop {
        if shutdown.load(Ordering::Acquire) {
            break;
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(_) => continue,
            Err(_) => break,
        }
    }
}
