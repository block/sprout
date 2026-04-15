//! Text-to-Speech pipeline for huddle agent voice output.
//!
//! Mental model:
//!
//! ```text
//! caller: pipeline.speak("Hello world. How are you?")
//!   → bounded sync_channel (TEXT_QUEUE_DEPTH = 8)
//!   → tts_worker thread (owns 1 Kokoro engine)
//!       1. Preprocess text
//!       2. Split into sentences
//!       3. Synthesize each sentence individually → f32 PCM
//!       4. Apply volume boost + fade in/out to each sentence
//!       5. Append each buffer to a single rodio Player (gapless playback)
//!       6. Wait for player.empty() before accepting next text item
//!   → tts_active = true while playing, false when idle
//!   → cancel flag: player.clear() + drain queue
//! ```
//!
//! Lookahead pipelining: synthesis of sentence N+1 begins immediately after
//! appending sentence N to the Player. Rodio queues buffers and plays them
//! sequentially — synthesis overlaps with playback for near-zero gaps.
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

use super::kokoro::{load_text_to_speech, load_voice_style, SAMPLE_RATE};
use super::preprocessing::{preprocess_for_tts, split_sentences};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum number of queued text items.
/// Prevents unbounded accumulation when the agent produces text faster than
/// TTS can play it. Excess items are dropped with a warning.
const TEXT_QUEUE_DEPTH: usize = 8;

/// How long the worker waits on the text channel before checking the shutdown flag.
const RECV_TIMEOUT: Duration = Duration::from_millis(100);

/// Kokoro ignores denoising steps (not a diffusion model). Kept for API compat.
const SYNTH_STEPS: usize = 1;

/// Synthesis speed multiplier. Slightly faster than natural speech.
const SYNTH_SPEED: f32 = 1.05;

/// Volume boost applied after synthesis — Kokoro output is normalized.
/// Start at 1.5 and tune empirically.
const VOLUME_BOOST: f32 = 1.5;

/// Fade in/out length in samples (8ms at 24kHz ≈ 192 samples).
/// Eliminates clicks/pops at sentence boundaries.
const FADE_SAMPLES: usize = (SAMPLE_RATE as f64 * 0.008) as usize;

/// Sentence-by-sentence synthesis for lower TTFA (≈200ms vs ≈600ms for 3-sentence batches).
const BATCH_SIZE: usize = 1;

/// Silence inserted between sentences by the TTS pipeline (seconds).
/// Injected as a silent buffer between each synthesized sentence chunk.
const INTER_SENTENCE_SILENCE: f32 = 0.1;

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
    /// Kept alive here so the Arc isn't dropped — the worker holds a clone.
    #[allow(dead_code)]
    cancel: Arc<AtomicBool>,
    /// Voice name (e.g. "af_heart"). Stored for future voice-switching support.
    #[allow(dead_code)]
    voice: String,
    /// Worker thread handle — taken on drop to join cleanly.
    thread: Option<thread::JoinHandle<()>>,
}

impl TtsPipeline {
    /// Spawn the TTS pipeline thread using the default voice.
    ///
    /// `model_dir` must contain the Kokoro model files:
    ///   `model_quantized.onnx`, `tokenizer.json`, `voices/<name>.bin`
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
        use super::kokoro::DEFAULT_VOICE;
        Self::new_with_voice(model_dir, tts_active, cancel, DEFAULT_VOICE)
    }

    /// Spawn the TTS pipeline thread with a specific voice name (e.g. `"af_heart"`, `"am_michael"`).
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

    /// Signal the worker thread to stop.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
    }

    /// Returns `true` if the worker thread has exited (init failure, crash, or normal exit).
    /// Used by hot-start to detect dead pipelines and clear them for retry.
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().map_or(true, |h| h.is_finished())
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
    // ── 1. Initialise Kokoro engine ───────────────────────────────────────────
    let model_dir_str = model_dir.to_string_lossy().to_string();

    let mut engine = match load_text_to_speech(&model_dir_str) {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "sprout-desktop: TTS Kokoro init failed (model_dir={}): {e}. TTS disabled.",
                model_dir.display()
            );
            drain_until_shutdown(text_rx, &shutdown);
            return;
        }
    };

    // ── 2. Load voice style ───────────────────────────────────────────────────
    let voice_path = model_dir.join(format!("{voice_name}.bin"));
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

    // Prime the audio output stream with a short silent buffer.
    // On macOS, CoreAudio initializes the output device lazily on first use.
    // Without this, the first real Player races against device startup and
    // player.empty() returns true before audio has started draining — causing
    // the first TTS message to be truncated after a few words.
    {
        use rodio::buffer::SamplesBuffer;
        let channels = NonZero::new(1u16).unwrap();
        let rate = NonZero::new(SAMPLE_RATE).unwrap();
        let silence = vec![0.0f32; SAMPLE_RATE as usize / 10]; // 100ms of silence
        let player = Player::connect_new(&sink_handle.mixer());
        player.append(SamplesBuffer::new(channels, rate, silence));
        // Wait for the silent buffer to drain — this ensures the output stream
        // is fully initialized before the main loop creates its first Player.
        while !player.empty() {
            thread::sleep(Duration::from_millis(10));
        }
    }

    // ── 4. Main loop ──────────────────────────────────────────────────────────
    loop {
        // Check shutdown/cancel before blocking (no player yet).
        if handle_cancel_or_shutdown(&cancel, &shutdown, &tts_active, &text_rx, None) {
            if shutdown.load(Ordering::Acquire) {
                break;
            }
            continue;
        }

        let raw_text = match text_rx.recv_timeout(RECV_TIMEOUT) {
            Ok(t) => t,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        // Check cancel again after unblocking — a cancel may have arrived
        // while we were waiting.
        if handle_cancel_or_shutdown(&cancel, &shutdown, &tts_active, &text_rx, None) {
            if shutdown.load(Ordering::Acquire) {
                break;
            }
            continue;
        }

        // Preprocess text.
        let text = preprocess_for_tts(&raw_text);
        if text.is_empty() {
            continue;
        }

        // Split into sentences. Each sentence is synthesized individually and
        // appended to the Player immediately — synthesis of sentence N+1 overlaps
        // with playback of sentence N (lookahead pipelining).
        let sentences: Vec<String> = split_sentences(&text)
            .into_iter()
            .filter(|s| !s.trim().is_empty())
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

        // Single persistent Player — all sentences append here, rodio plays
        // them gaplessly without per-sentence device setup overhead.
        let player = Player::connect_new(&sink_handle.mixer());
        // NOTE: tts_active is set AFTER the first player.append(), not before.
        // Setting it before synthesis would cause STT to discard user speech
        // during the synthesis window as "echo" even though no audio is
        // actually playing yet. See crossfire review C3.
        let mut first_append = true;

        // Lookahead pipeline: synthesize each sentence and append immediately.
        // Rodio queues buffers sequentially — synthesis of the next sentence
        // overlaps with playback of the current one.
        let silence_samples = (INTER_SENTENCE_SILENCE * SAMPLE_RATE as f32) as usize;
        let silence_buf = vec![0.0f32; silence_samples];
        for sentence in &sentences {
            if handle_cancel_or_shutdown(&cancel, &shutdown, &tts_active, &text_rx, Some(&player)) {
                break;
            }

            let text = sentence.trim();
            if text.is_empty() {
                continue;
            }

            match engine.synth_chunk(text, "en", &style, SYNTH_STEPS, SYNTH_SPEED) {
                Ok(samples) if !samples.is_empty() => {
                    let mut boosted: Vec<f32> = samples
                        .iter()
                        .map(|&s| (s * VOLUME_BOOST).clamp(-1.0, 1.0))
                        .collect();
                    apply_fades(&mut boosted);
                    player.append(SamplesBuffer::new(channels, rate, boosted));
                    // Insert inter-sentence silence after each synthesized chunk.
                    player.append(SamplesBuffer::new(channels, rate, silence_buf.clone()));
                    if first_append {
                        tts_active.store(true, Ordering::Release);
                        first_append = false;
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("sprout-desktop: TTS synth failed: {e}");
                }
            }
        }

        // Wait for all queued audio to finish playing.
        loop {
            if handle_cancel_or_shutdown(&cancel, &shutdown, &tts_active, &text_rx, Some(&player)) {
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

/// Check for cancel or shutdown. Returns `true` if the caller should break/continue.
/// On cancel: drains the text queue and clears the cancel flag.
fn handle_cancel_or_shutdown(
    cancel: &AtomicBool,
    shutdown: &AtomicBool,
    tts_active: &AtomicBool,
    text_rx: &mpsc::Receiver<String>,
    player: Option<&rodio::Player>,
) -> bool {
    if shutdown.load(Ordering::Acquire) {
        if let Some(p) = player {
            p.clear();
        }
        tts_active.store(false, Ordering::Release);
        return true;
    }
    if cancel.load(Ordering::Acquire) {
        if let Some(p) = player {
            p.clear();
        }
        while text_rx.try_recv().is_ok() {}
        cancel.store(false, Ordering::Release);
        tts_active.store(false, Ordering::Release);
        return true;
    }
    false
}

/// Apply a short linear fade-in at the start and fade-out at the end of `samples`.
///
/// Uses `FADE_SAMPLES` (8ms) or half the buffer length, whichever is smaller.
/// Eliminates clicks/pops at sentence boundaries.
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

// BATCH_SIZE is used implicitly (one sentence per iteration). Suppress dead_code
// lint since it documents the design intent.
const _: () = assert!(BATCH_SIZE == 1);
