//! Text-to-Speech pipeline for huddle agent voice output.
//!
//! Mental model:
//!
//! ```text
//! caller: pipeline.speak("Hello")
//!   → bounded sync_channel (TEXT_QUEUE_DEPTH)
//!   → tts_worker thread
//!       sherpa-onnx Kokoro: text → f32 samples
//!       rodio Player: play samples (blocks until done)
//!       tts_active = true while playing, false when idle
//!   → caller: pipeline.cancel()  → clears queue + stops current playback
//! ```
//!
//! The worker runs on a dedicated `std::thread` (not async) because
//! sherpa-onnx is CPU-bound and not Send-safe across await points.
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

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum number of queued text items.
/// Prevents unbounded accumulation when the agent produces text faster than
/// TTS can play it. Excess items are dropped with a warning.
const TEXT_QUEUE_DEPTH: usize = 8;

/// How long the worker waits on the text channel before checking the shutdown flag.
const RECV_TIMEOUT: Duration = Duration::from_millis(100);

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
    /// Worker thread handle — taken on drop to join cleanly.
    thread: Option<thread::JoinHandle<()>>,
}

impl TtsPipeline {
    /// Spawn the TTS pipeline thread.
    ///
    /// `model_dir` must contain the Kokoro model files:
    ///   `kokoro.onnx`, `voices.bin`, `tokens.txt`, `espeak-ng-data/`
    ///
    /// `tts_active` is set to `true` while audio is playing and `false` when idle.
    /// Pass the same `Arc` to the STT pipeline to gate microphone input.
    ///
    /// Returns `Err` only if the thread cannot be spawned (OS error).
    /// If model files are missing, the worker logs and exits cleanly.
    pub fn new(model_dir: PathBuf, tts_active: Arc<AtomicBool>) -> Result<Self, String> {
        let (text_tx, text_rx) = mpsc::sync_channel::<String>(TEXT_QUEUE_DEPTH);
        let shutdown = Arc::new(AtomicBool::new(false));
        let cancel = Arc::new(AtomicBool::new(false));

        let shutdown_worker = Arc::clone(&shutdown);
        let cancel_worker = Arc::clone(&cancel);
        let tts_active_worker = Arc::clone(&tts_active);

        let handle = thread::Builder::new()
            .name("tts-worker".into())
            .spawn(move || {
                tts_worker(
                    model_dir,
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
    text_rx: mpsc::Receiver<String>,
    tts_active: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
) {
    // ── 1. Initialise sherpa-onnx Kokoro TTS ──────────────────────────────────
    use sherpa_onnx::{
        GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig,
        OfflineTtsModelConfig,
    };

    let model_dir_str = model_dir.to_string_lossy().into_owned();

    let kokoro_cfg = OfflineTtsKokoroModelConfig {
        model: Some(format!("{model_dir_str}/kokoro.onnx")),
        voices: Some(format!("{model_dir_str}/voices.bin")),
        tokens: Some(format!("{model_dir_str}/tokens.txt")),
        data_dir: Some(format!("{model_dir_str}/espeak-ng-data")),
        length_scale: 1.0,
        lang: Some("en-us".into()),
        ..Default::default()
    };

    let model_cfg = OfflineTtsModelConfig {
        kokoro: kokoro_cfg,
        num_threads: 1,
        ..Default::default()
    };

    let tts_cfg = OfflineTtsConfig {
        model: model_cfg,
        max_num_sentences: 1,
        ..Default::default()
    };

    let tts = match OfflineTts::create(&tts_cfg) {
        Some(t) => t,
        None => {
            eprintln!(
                "sprout-desktop: TTS Kokoro init failed (model_dir={}). TTS disabled.",
                model_dir.display()
            );
            drain_text_channel(text_rx, &shutdown);
            return;
        }
    };

    let gen_cfg = GenerationConfig {
        speed: 1.0,
        sid: 0,
        ..Default::default()
    };

    // ── 2. Initialise rodio output device ─────────────────────────────────────
    use rodio::{DeviceSinkBuilder, Player};

    let sink_handle = match DeviceSinkBuilder::open_default_sink() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("sprout-desktop: TTS audio output failed: {e}. TTS disabled.");
            drain_text_channel(text_rx, &shutdown);
            return;
        }
    };

    // ── 3. Main loop ──────────────────────────────────────────────────────────
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

        // Generate audio via Kokoro.
        let audio = match tts.generate_with_config(
            &text,
            &gen_cfg,
            None::<fn(&[f32], f32) -> bool>,
        ) {
            Some(a) => a,
            None => {
                eprintln!("sprout-desktop: TTS generate_with_config returned None for: {text:?}");
                continue;
            }
        };

        let samples = audio.samples();
        if samples.is_empty() {
            continue;
        }

        let sample_rate = audio.sample_rate() as u32;

        // Build rodio SamplesBuffer from f32 samples.
        use rodio::buffer::SamplesBuffer;
        let channels = NonZero::new(1u16).expect("1 is nonzero");
        let rate = NonZero::new(sample_rate).unwrap_or(NonZero::new(24_000u32).unwrap());
        let buf = SamplesBuffer::new(channels, rate, samples.to_vec());

        // Play via rodio Player (blocks until playback completes).
        tts_active.store(true, Ordering::Release);
        let player = Player::connect_new(&sink_handle.mixer());
        player.append(buf);

        // Wait for playback to finish, polling cancel/shutdown every 50 ms.
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
