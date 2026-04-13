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
//!       3. Batch sentences in groups of BATCH_SIZE → synth_batch() → f32 PCM
//!       4. Apply volume boost + fade in/out to each batch
//!       5. Append all buffers to a single rodio Player (gapless playback)
//!       6. Wait for player.empty() before accepting next text item
//!   → tts_active = true while playing, false when idle
//!   → cancel flag: player.clear() + drain queue
//! ```
//!
//! Supertonic synthesis is ~167× faster than real-time. Batching 3 sentences
//! per engine.call() improves prosody (more context) and eliminates
//! inter-sentence gaps via a single persistent rodio Player.
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

use super::preprocessing::{preprocess_for_tts, split_sentences};
use super::supertonic::{
    self, load_text_to_speech, load_voice_style, Style, TextToSpeech, SAMPLE_RATE,
};

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

/// Volume boost applied after synthesis — Supertonic output is quiet.
const VOLUME_BOOST: f32 = 2.5;

/// Fade in/out length in samples (8ms at 44.1kHz ≈ 352 samples).
/// Eliminates clicks/pops at batch boundaries.
const FADE_SAMPLES: usize = (SAMPLE_RATE as f64 * 0.008) as usize;

/// Number of sentences batched per engine.call() invocation.
/// More context → better prosody; synthesis is fast enough that this is free.
const BATCH_SIZE: usize = 3;

/// Silence inserted between batched sentences by the Supertonic engine (seconds).
const INTER_SENTENCE_SILENCE: f32 = 0.15;

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
    ///
    /// `cancel` is the shared barge-in flag from `HuddleState.tts_cancel`. Pass the
    /// same `Arc` to the STT pipeline so both sides reference the same flag for the
    /// entire huddle session — no stale references after pipeline restarts.
    pub fn new(
        model_dir: PathBuf,
        tts_active: Arc<AtomicBool>,
        cancel: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        Self::new_with_voice(model_dir, tts_active, cancel, supertonic::DEFAULT_VOICE)
    }

    /// Spawn the TTS pipeline thread with a specific voice name (e.g. `"F1"`, `"M3"`).
    pub fn new_with_voice(
        model_dir: PathBuf,
        tts_active: Arc<AtomicBool>,
        cancel: Arc<AtomicBool>,
        voice: &str,
    ) -> Result<Self, String> {
        let (text_tx, text_rx) = mpsc::sync_channel::<String>(TEXT_QUEUE_DEPTH);
        let shutdown = Arc::new(AtomicBool::new(false));
        // cancel is passed in from HuddleState.tts_cancel — shared with STT for barge-in.

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
        self.text_tx.try_send(text).map_err(|e| {
            eprintln!("sprout-desktop: TTS queue saturated, dropping message: {e}");
            format!("TTS queue full, dropping: {e}")
        })
    }

    /// Barge-in: cancel current speech and discard queued items.
    ///
    /// Sets the cancel flag. The worker will drain the queue and stop the
    /// current rodio Player on its next iteration.
    ///
    /// Currently unused — barge-in is triggered by the STT pipeline setting
    /// `tts_cancel` directly via the shared `Arc<AtomicBool>`. Retained as
    /// public API for future callers (e.g., explicit "stop speaking" button).
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
            drain_until_shutdown(text_rx, &shutdown);
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
            drain_until_shutdown(text_rx, &shutdown);
            return;
        }
    };

    // ── 3. Initialise rodio output device ─────────────────────────────────────
    use rodio::{DeviceSinkBuilder, Player};

    let sink_handle = match DeviceSinkBuilder::open_default_sink() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("sprout-desktop: TTS audio output failed: {e}. TTS disabled.");
            drain_until_shutdown(text_rx, &shutdown);
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

        // Split into sentences, batch in groups of BATCH_SIZE for better
        // prosody and gapless playback via a single persistent Player.
        let sentences: Vec<String> = split_sentences(&text)
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();

        if sentences.is_empty() {
            continue;
        }

        use rodio::buffer::SamplesBuffer;
        let channels = match NonZero::new(1u16) {
            Some(c) => c,
            None => {
                eprintln!("sprout-desktop: TTS channel count invariant violated");
                break;
            }
        };
        let rate = match NonZero::new(SAMPLE_RATE) {
            Some(r) => r,
            None => {
                eprintln!("sprout-desktop: TTS sample rate invariant violated");
                break;
            }
        };

        // Single persistent Player — all batches append here, rodio plays
        // them gaplessly without per-batch device setup overhead.
        let player = Player::connect_new(&sink_handle.mixer());
        tts_active.store(true, Ordering::Release);

        for chunk in sentences.chunks(BATCH_SIZE) {
            if cancel.load(Ordering::Acquire) || shutdown.load(Ordering::Acquire) {
                player.clear();
                while text_rx.try_recv().is_ok() {}
                cancel.store(false, Ordering::Release);
                break;
            }

            if let Some(samples) = synth_batch(&mut engine, chunk, &style) {
                let buf = SamplesBuffer::new(channels, rate, samples);
                player.append(buf);
            }
        }

        // Wait for all queued audio to finish playing.
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

        tts_active.store(false, Ordering::Release);

        if shutdown.load(Ordering::Acquire) {
            break;
        }
    }

    tts_active.store(false, Ordering::Release);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Synthesize a batch of sentences in a single engine call.
///
/// Sentences are joined with a space so Supertonic sees full context for
/// better prosody. `silence_secs=INTER_SENTENCE_SILENCE` lets the engine
/// insert natural pauses between sentences internally.
///
/// After synthesis: volume boost (×VOLUME_BOOST, clamped) + 8ms fade in/out
/// to eliminate clicks at batch boundaries.
fn synth_batch(engine: &mut TextToSpeech, sentences: &[String], style: &Style) -> Option<Vec<f32>> {
    let text = sentences.join(" ");
    match engine.call(
        &text,
        "en",
        style,
        SYNTH_STEPS,
        SYNTH_SPEED,
        INTER_SENTENCE_SILENCE,
    ) {
        Ok(samples) if !samples.is_empty() => {
            // Volume boost — Supertonic output is quiet.
            let mut boosted: Vec<f32> = samples
                .iter()
                .map(|&s| (s * VOLUME_BOOST).clamp(-1.0, 1.0))
                .collect();
            // Fade in/out to eliminate clicks at batch boundaries.
            apply_fades(&mut boosted);
            Some(boosted)
        }
        Ok(_) => None,
        Err(e) => {
            eprintln!("sprout-desktop: TTS synth failed: {e}");
            None
        }
    }
}

/// Apply a short linear fade-in at the start and fade-out at the end of `samples`.
///
/// Uses `FADE_SAMPLES` (8ms) or half the buffer length, whichever is smaller.
/// Eliminates clicks/pops at batch boundaries.
fn apply_fades(samples: &mut Vec<f32>) {
    let len = samples.len();
    let fade = FADE_SAMPLES.min(len / 2);
    // Fade in: ramp from 0 → 1 over `fade` samples.
    for i in 0..fade {
        samples[i] *= i as f32 / fade as f32;
    }
    // Fade out: ramp from 1 → 0 over the last `fade` samples.
    for i in 0..fade {
        samples[len - 1 - i] *= i as f32 / fade as f32;
    }
}

// drain_until_shutdown lives in super (huddle/mod.rs) — shared with stt.rs.
use super::drain_until_shutdown;
