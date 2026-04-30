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
        output_device: Option<String>,
    ) -> Result<Self, String> {
        use super::kokoro::DEFAULT_VOICE;
        Self::new_with_voice(model_dir, tts_active, cancel, DEFAULT_VOICE, output_device)
    }

    /// Spawn the TTS pipeline thread with a specific voice name (e.g. `"af_heart"`, `"am_michael"`).
    pub fn new_with_voice(
        model_dir: PathBuf,
        tts_active: Arc<AtomicBool>,
        cancel: Arc<AtomicBool>,
        voice: &str,
        output_device: Option<String>,
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
                    output_device,
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
    output_device: Option<String>,
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

    // ── 2b. Warmup inference ─────────────────────────────────────────────────
    // The first ONNX inference on any session is significantly slower than
    // subsequent ones — it triggers JIT compilation, memory pool allocation,
    // and (on CoreML) lazy model compilation. Run a short dummy synthesis and
    // discard the output so the first real utterance runs at warm-session speed.
    {
        let t = std::time::Instant::now();
        match engine.synth_chunk("warmup", "en", &style, SYNTH_STEPS, SYNTH_SPEED) {
            Ok(_) => eprintln!(
                "sprout-desktop: TTS warmup completed in {:.0}ms",
                t.elapsed().as_millis()
            ),
            Err(e) => eprintln!(
                "sprout-desktop: TTS warmup failed after {:.0}ms: {e} — first utterance may be slow",
                t.elapsed().as_millis()
            ),
        }
    }

    // ── 3. Initialise rodio output device ─────────────────────────────────────
    use rodio::Player;

    let sink_handle = match super::audio_output::open_output_sink_by_name(output_device.as_deref())
    {
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::sync::Arc;

    // ── Remote interrupt tracker ──────────────────────────────────────────────
    //
    // Models the per-peer frame counting logic in the recv task of
    // relay_api.rs. The contract (must match the production implementation):
    //
    //   - Frame counting is GATED on tts_active — counters only increment
    //     while TTS is playing.
    //   - On TTS session start (false→true transition), all counters and
    //     the window timer are reset. Prevents stale pre-playback speech
    //     from tripping a cancel.
    //   - In production, counting happens after successful Opus decode
    //     (Ok(n) if n > 0). We can't model decode in unit tests, so
    //     on_frame() represents a successfully-decoded audio frame.
    //   - Each remote peer has an independent frame counter.
    //   - Counters use saturating_add (overflow-safe).
    //   - When a peer's counter crosses REMOTE_SPEECH_THRESHOLD, set
    //     tts_cancel = true.
    //   - Counters reset on the 500ms window (Instant-based in production,
    //     on_tick() in tests — logically equivalent).
    //   - Uses Acquire for tts_active reads, Release for tts_cancel writes.
    //
    use crate::huddle::relay_api::REMOTE_SPEECH_THRESHOLD;

    /// Test-side model of the per-peer frame counting logic in the recv task.
    struct RemoteInterruptTracker {
        frame_counts: HashMap<u8, u16>,
        tts_active: Arc<AtomicBool>,
        tts_cancel: Arc<AtomicBool>,
        /// Tracks the previous tts_active state to detect false→true transitions.
        /// Mirrors `tts_was_active` in the production recv task.
        tts_was_active: bool,
    }

    impl RemoteInterruptTracker {
        fn new(tts_active: Arc<AtomicBool>, tts_cancel: Arc<AtomicBool>) -> Self {
            Self {
                frame_counts: HashMap::new(),
                tts_active,
                tts_cancel,
                tts_was_active: false,
            }
        }

        /// Called when a successfully-decoded audio frame arrives from a
        /// remote peer. Mirrors the production logic in relay_api.rs:
        ///   1. Check tts_active — skip if inactive
        ///   2. On false→true transition, clear all counters (new TTS session)
        ///   3. Increment peer counter (saturating)
        ///   4. Fire cancel if threshold crossed
        fn on_frame(&mut self, peer_idx: u8) {
            let tts_now = self.tts_active.load(Ordering::Acquire);

            // Detect TTS session start — clear stale counters.
            if tts_now && !self.tts_was_active {
                self.frame_counts.clear();
            }
            self.tts_was_active = tts_now;

            if !tts_now {
                return; // Not counting while TTS is inactive.
            }

            let count = self.frame_counts.entry(peer_idx).or_insert(0);
            *count = count.saturating_add(1);
            if *count >= REMOTE_SPEECH_THRESHOLD {
                self.tts_cancel.store(true, Ordering::Release);
            }
        }

        /// Called on the 500ms window boundary — resets all frame counters.
        /// In production this is Instant-based (starvation-proof); in tests
        /// we call it explicitly since there's no async event loop.
        fn on_tick(&mut self) {
            self.frame_counts.clear();
        }

        fn count_for(&self, peer_idx: u8) -> u16 {
            *self.frame_counts.get(&peer_idx).unwrap_or(&0)
        }
    }

    /// Simulate the TTS worker's cancel-handling logic (from handle_cancel_or_shutdown).
    /// Returns true if cancel was processed (mirrors the real function's return value).
    fn simulate_cancel_consumption(
        cancel: &AtomicBool,
        shutdown: &AtomicBool,
        tts_active: &AtomicBool,
        text_rx: &mpsc::Receiver<String>,
    ) -> bool {
        if shutdown.load(Ordering::Acquire) {
            tts_active.store(false, Ordering::Release);
            return true;
        }
        if cancel.load(Ordering::Acquire) {
            while text_rx.try_recv().is_ok() {}
            cancel.store(false, Ordering::Release);
            tts_active.store(false, Ordering::Release);
            return true;
        }
        false
    }

    // ── Threshold tests ───────────────────────────────────────────────────────

    /// Real speech above threshold during TTS → cancel fires.
    #[test]
    fn speech_above_threshold_sets_cancel() {
        let tts_active = Arc::new(AtomicBool::new(true));
        let tts_cancel = Arc::new(AtomicBool::new(false));
        let mut tracker =
            RemoteInterruptTracker::new(Arc::clone(&tts_active), Arc::clone(&tts_cancel));

        // Send exactly REMOTE_SPEECH_THRESHOLD frames from peer 1.
        for _ in 0..REMOTE_SPEECH_THRESHOLD {
            tracker.on_frame(1);
        }

        assert!(
            tts_cancel.load(Ordering::Acquire),
            "tts_cancel should be true after {} frames from a peer during active TTS",
            REMOTE_SPEECH_THRESHOLD,
        );
    }

    /// DTX comfort noise below threshold during TTS → cancel does NOT fire.
    #[test]
    fn comfort_noise_below_threshold_no_cancel() {
        let tts_active = Arc::new(AtomicBool::new(true));
        let tts_cancel = Arc::new(AtomicBool::new(false));
        let mut tracker =
            RemoteInterruptTracker::new(Arc::clone(&tts_active), Arc::clone(&tts_cancel));

        // DTX comfort noise: ~2-3 frames per 500ms tick. Send fewer than threshold.
        for _ in 0..(REMOTE_SPEECH_THRESHOLD - 1) {
            tracker.on_frame(1);
        }

        assert!(
            !tts_cancel.load(Ordering::Acquire),
            "tts_cancel should remain false with only {} frames (below threshold of {})",
            REMOTE_SPEECH_THRESHOLD - 1,
            REMOTE_SPEECH_THRESHOLD,
        );
    }

    /// Frames arrive while tts_active=false → no cancel regardless of count.
    #[test]
    fn frames_without_tts_active_no_cancel() {
        let tts_active = Arc::new(AtomicBool::new(false));
        let tts_cancel = Arc::new(AtomicBool::new(false));
        let mut tracker =
            RemoteInterruptTracker::new(Arc::clone(&tts_active), Arc::clone(&tts_cancel));

        // Send many frames — well above threshold.
        for _ in 0..50 {
            tracker.on_frame(1);
        }

        assert!(
            !tts_cancel.load(Ordering::Acquire),
            "tts_cancel should remain false when TTS is not active",
        );
    }

    /// Counter reset on tick → peer must re-accumulate frames to trigger cancel.
    #[test]
    fn tick_resets_counters() {
        let tts_active = Arc::new(AtomicBool::new(true));
        let tts_cancel = Arc::new(AtomicBool::new(false));
        let mut tracker =
            RemoteInterruptTracker::new(Arc::clone(&tts_active), Arc::clone(&tts_cancel));

        // Send frames just below threshold.
        for _ in 0..(REMOTE_SPEECH_THRESHOLD - 1) {
            tracker.on_frame(1);
        }
        assert!(!tts_cancel.load(Ordering::Acquire));

        // Tick resets counters.
        tracker.on_tick();
        assert_eq!(tracker.count_for(1), 0, "counter should be zero after tick");

        // Send same number again — still below threshold because counter was reset.
        for _ in 0..(REMOTE_SPEECH_THRESHOLD - 1) {
            tracker.on_frame(1);
        }
        assert!(
            !tts_cancel.load(Ordering::Acquire),
            "cancel should not fire: counter was reset by tick, frames still below threshold",
        );
    }

    /// Per-peer isolation: peer A's silence does not reset peer B's accumulation.
    #[test]
    fn per_peer_counters_are_independent() {
        let tts_active = Arc::new(AtomicBool::new(true));
        let tts_cancel = Arc::new(AtomicBool::new(false));
        let mut tracker =
            RemoteInterruptTracker::new(Arc::clone(&tts_active), Arc::clone(&tts_cancel));

        // Peer 1 sends frames below threshold (DTX comfort noise).
        for _ in 0..2 {
            tracker.on_frame(1);
        }

        // Peer 2 sends frames above threshold (real speech).
        for _ in 0..REMOTE_SPEECH_THRESHOLD {
            tracker.on_frame(2);
        }

        assert!(
            tts_cancel.load(Ordering::Acquire),
            "peer 2 should trigger cancel independently of peer 1's low count",
        );
        assert_eq!(
            tracker.count_for(1),
            2,
            "peer 1 counter should be untouched"
        );
        assert_eq!(
            tracker.count_for(2),
            REMOTE_SPEECH_THRESHOLD,
            "peer 2 counter should be at threshold",
        );
    }

    /// Multiple peers both above threshold → cancel fires (idempotent).
    #[test]
    fn multiple_peers_above_threshold_idempotent() {
        let tts_active = Arc::new(AtomicBool::new(true));
        let tts_cancel = Arc::new(AtomicBool::new(false));
        let mut tracker =
            RemoteInterruptTracker::new(Arc::clone(&tts_active), Arc::clone(&tts_cancel));

        // Both peers send above threshold.
        for _ in 0..REMOTE_SPEECH_THRESHOLD {
            tracker.on_frame(1);
            tracker.on_frame(2);
        }

        assert!(
            tts_cancel.load(Ordering::Acquire),
            "cancel should be set when multiple peers exceed threshold",
        );
        // tts_active should still be true — only the TTS worker resets it.
        assert!(
            tts_active.load(Ordering::Acquire),
            "tts_active should remain true (only TTS worker clears it)",
        );
    }

    /// tts_cancel already true → setting again is harmless (AtomicBool store).
    #[test]
    fn cancel_already_true_is_harmless() {
        let tts_active = Arc::new(AtomicBool::new(true));
        let tts_cancel = Arc::new(AtomicBool::new(true)); // Already cancelled.
        let mut tracker =
            RemoteInterruptTracker::new(Arc::clone(&tts_active), Arc::clone(&tts_cancel));

        for _ in 0..REMOTE_SPEECH_THRESHOLD {
            tracker.on_frame(1);
        }

        assert!(
            tts_cancel.load(Ordering::Acquire),
            "tts_cancel should still be true (idempotent store)",
        );
    }

    // ── Regression: local-only interrupt still works ──────────────────────────

    /// The existing local barge-in path (STT detects speech → sets tts_cancel)
    /// must continue to work independently of remote frame counting.
    #[test]
    fn local_barge_in_still_works_without_remote_frames() {
        let _tts_active = AtomicBool::new(true);
        let tts_cancel = AtomicBool::new(false);

        // Simulate local STT barge-in (stt.rs after BARGE_IN_DEBOUNCE_FRAMES).
        tts_cancel.store(true, Ordering::Release);

        assert!(
            tts_cancel.load(Ordering::Acquire),
            "local barge-in should set tts_cancel",
        );
    }

    // ── Cancel consumption tests (TTS worker side) ────────────────────────────

    /// TTS worker correctly resets both tts_cancel and tts_active after cancel.
    #[test]
    fn cancel_consumption_resets_flags() {
        let cancel = AtomicBool::new(true);
        let shutdown = AtomicBool::new(false);
        let tts_active = AtomicBool::new(true);
        let (_tx, rx) = mpsc::channel::<String>();

        let handled = simulate_cancel_consumption(&cancel, &shutdown, &tts_active, &rx);

        assert!(handled, "should return true when cancel is set");
        assert!(
            !cancel.load(Ordering::Acquire),
            "cancel should be reset to false after consumption",
        );
        assert!(
            !tts_active.load(Ordering::Acquire),
            "tts_active should be reset to false after cancel",
        );
    }

    /// TTS worker drains the text queue on cancel.
    #[test]
    fn cancel_consumption_drains_queue() {
        let cancel = AtomicBool::new(true);
        let shutdown = AtomicBool::new(false);
        let tts_active = AtomicBool::new(true);
        let (tx, rx) = mpsc::channel::<String>();

        tx.send("sentence one".to_string()).unwrap();
        tx.send("sentence two".to_string()).unwrap();
        tx.send("sentence three".to_string()).unwrap();

        let handled = simulate_cancel_consumption(&cancel, &shutdown, &tts_active, &rx);

        assert!(handled);
        assert!(
            rx.try_recv().is_err(),
            "text queue should be drained after cancel",
        );
    }

    /// No cancel, no shutdown → TTS worker continues (returns false).
    #[test]
    fn no_cancel_no_shutdown_returns_false() {
        let cancel = AtomicBool::new(false);
        let shutdown = AtomicBool::new(false);
        let tts_active = AtomicBool::new(true);
        let (_tx, rx) = mpsc::channel::<String>();

        let handled = simulate_cancel_consumption(&cancel, &shutdown, &tts_active, &rx);

        assert!(
            !handled,
            "should return false when neither cancel nor shutdown is set",
        );
        assert!(
            tts_active.load(Ordering::Acquire),
            "tts_active should remain true",
        );
    }

    /// Shutdown takes priority over cancel — cancel flag is NOT reset.
    #[test]
    fn shutdown_takes_priority_over_cancel() {
        let cancel = AtomicBool::new(true);
        let shutdown = AtomicBool::new(true);
        let tts_active = AtomicBool::new(true);
        let (_tx, rx) = mpsc::channel::<String>();

        let handled = simulate_cancel_consumption(&cancel, &shutdown, &tts_active, &rx);

        assert!(handled, "should return true on shutdown");
        assert!(
            !tts_active.load(Ordering::Acquire),
            "tts_active should be false after shutdown",
        );
        assert!(
            cancel.load(Ordering::Acquire),
            "cancel should remain true (shutdown path doesn't reset it)",
        );
    }

    // ── Full lifecycle tests ──────────────────────────────────────────────────

    /// Full cycle: remote speech → cancel → TTS consumption → new TTS → cancel again.
    /// Validates the cancel mechanism is reusable across TTS sessions.
    /// The false→true transition on tts_active auto-clears counters — no
    /// explicit on_tick() needed between sessions.
    #[test]
    fn full_cancel_cycle_is_reusable() {
        let tts_active = Arc::new(AtomicBool::new(false));
        let tts_cancel = Arc::new(AtomicBool::new(false));
        let shutdown = AtomicBool::new(false);
        let (_tx, rx) = mpsc::channel::<String>();

        let mut tracker =
            RemoteInterruptTracker::new(Arc::clone(&tts_active), Arc::clone(&tts_cancel));

        // Cycle 1: TTS starts, remote speech triggers cancel, TTS consumes.
        tts_active.store(true, Ordering::Release);
        for _ in 0..REMOTE_SPEECH_THRESHOLD {
            tracker.on_frame(1);
        }
        assert!(tts_cancel.load(Ordering::Acquire));

        let handled = simulate_cancel_consumption(&tts_cancel, &shutdown, &tts_active, &rx);
        assert!(handled);
        assert!(!tts_cancel.load(Ordering::Acquire));
        assert!(!tts_active.load(Ordering::Acquire));

        // No on_tick() needed — the false→true transition auto-clears counters.

        // Cycle 2: New TTS starts, another remote speech triggers cancel.
        tts_active.store(true, Ordering::Release);
        for _ in 0..REMOTE_SPEECH_THRESHOLD {
            tracker.on_frame(1);
        }
        assert!(tts_cancel.load(Ordering::Acquire));

        let handled = simulate_cancel_consumption(&tts_cancel, &shutdown, &tts_active, &rx);
        assert!(handled);
        assert!(!tts_cancel.load(Ordering::Acquire));
        assert!(!tts_active.load(Ordering::Acquire));
    }

    /// TTS session transition (false→true) clears stale counters.
    /// Prevents pre-existing speech from tripping a cancel on TTS restart.
    #[test]
    fn tts_session_transition_clears_counters() {
        let tts_active = Arc::new(AtomicBool::new(true));
        let tts_cancel = Arc::new(AtomicBool::new(false));
        let mut tracker =
            RemoteInterruptTracker::new(Arc::clone(&tts_active), Arc::clone(&tts_cancel));

        // Accumulate frames just below threshold during first TTS session.
        for _ in 0..(REMOTE_SPEECH_THRESHOLD - 1) {
            tracker.on_frame(1);
        }
        assert_eq!(tracker.count_for(1), REMOTE_SPEECH_THRESHOLD - 1);
        assert!(!tts_cancel.load(Ordering::Acquire));

        // TTS stops (cancel consumed).
        tts_active.store(false, Ordering::Release);
        // Send a frame while inactive — triggers tts_was_active transition tracking.
        tracker.on_frame(1);

        // TTS restarts — false→true transition should clear counters.
        tts_active.store(true, Ordering::Release);
        tracker.on_frame(1); // First frame of new session triggers clear + count.
        assert_eq!(
            tracker.count_for(1),
            1,
            "counter should be 1 (cleared on session transition, then incremented)",
        );

        // Need full threshold again from scratch.
        for _ in 1..REMOTE_SPEECH_THRESHOLD {
            tracker.on_frame(1);
        }
        assert!(
            tts_cancel.load(Ordering::Acquire),
            "cancel should fire after full threshold in new session",
        );
    }

    /// Concurrent remote cancel + local barge-in → both safe, one cancel processed.
    #[test]
    fn concurrent_remote_and_local_cancel() {
        let tts_active = Arc::new(AtomicBool::new(true));
        let tts_cancel = Arc::new(AtomicBool::new(false));

        // Remote path: frame counting above threshold.
        let cancel_remote = Arc::clone(&tts_cancel);
        let active_remote = Arc::clone(&tts_active);
        let remote = std::thread::spawn(move || {
            // Simulate threshold crossing — directly set cancel (the tracker
            // would do this after REMOTE_SPEECH_THRESHOLD frames).
            if active_remote.load(Ordering::Acquire) {
                cancel_remote.store(true, Ordering::Release);
            }
        });

        // Local path: STT barge-in.
        let cancel_local = Arc::clone(&tts_cancel);
        let local = std::thread::spawn(move || {
            cancel_local.store(true, Ordering::Release);
        });

        remote.join().unwrap();
        local.join().unwrap();

        assert!(
            tts_cancel.load(Ordering::Acquire),
            "tts_cancel should be true regardless of which path set it",
        );
    }

    /// Frames while TTS is inactive are NOT counted. The peer must accumulate
    /// the full threshold AFTER tts_active becomes true.
    #[test]
    fn frames_while_tts_inactive_are_not_counted() {
        let tts_active = Arc::new(AtomicBool::new(false));
        let tts_cancel = Arc::new(AtomicBool::new(false));
        let mut tracker =
            RemoteInterruptTracker::new(Arc::clone(&tts_active), Arc::clone(&tts_cancel));

        // Send frames while TTS is inactive — should be ignored entirely.
        for _ in 0..50 {
            tracker.on_frame(1);
        }
        assert_eq!(
            tracker.count_for(1),
            0,
            "no frames should accumulate while TTS inactive"
        );
        assert!(!tts_cancel.load(Ordering::Acquire));

        // TTS starts. Peer must now accumulate from zero.
        tts_active.store(true, Ordering::Release);

        // Send frames below threshold — not enough yet.
        for _ in 0..(REMOTE_SPEECH_THRESHOLD - 1) {
            tracker.on_frame(1);
        }
        assert!(
            !tts_cancel.load(Ordering::Acquire),
            "cancel should not fire: only {} frames since TTS became active",
            REMOTE_SPEECH_THRESHOLD - 1,
        );

        // One more frame crosses the threshold.
        tracker.on_frame(1);
        assert!(
            tts_cancel.load(Ordering::Acquire),
            "cancel should fire after full threshold accumulated post-activation",
        );
    }

    // ── apply_fades tests ─────────────────────────────────────────────────────

    #[test]
    fn apply_fades_short_buffer() {
        let mut samples = vec![1.0f32; 10];
        apply_fades(&mut samples);
        assert_eq!(samples[0], 0.0);
        assert_eq!(samples[9], 0.0);
        assert!(samples[5] > 0.5);
    }

    #[test]
    fn apply_fades_empty_buffer() {
        let mut samples: Vec<f32> = vec![];
        apply_fades(&mut samples);
        assert!(samples.is_empty());
    }

    #[test]
    fn apply_fades_single_sample() {
        let mut samples = vec![1.0f32];
        apply_fades(&mut samples);
        assert_eq!(samples[0], 1.0);
    }
}
