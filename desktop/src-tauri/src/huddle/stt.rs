//! Speech-to-Text pipeline for huddle voice transcription.
//!
//! Mental model:
//!
//! ```text
//! AudioWorklet (48 kHz f32 PCM)
//!   → push_audio_pcm (Tauri cmd)
//!   → SttPipeline::push_audio  [bounded sync_channel]
//!   → stt_worker thread
//!       rubato: 48 kHz → 16 kHz mono
//!       earshot VAD: accumulate speech frames
//!       sherpa-onnx Moonshine: transcribe on silence
//!   → text_rx  [mpsc channel]
//!   → tokio task (start_stt_pipeline)
//!       builds kind:9 event → relay
//! ```
//!
//! The worker runs on a dedicated `std::thread` (not async) because
//! sherpa-onnx is CPU-bound and not Send-safe across await points.

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

// ── Public pipeline handle ────────────────────────────────────────────────────

/// Bounded audio queue capacity.
/// 100 ms batches at 48 kHz ≈ 19 KB each → 50 slots ≈ 5 s / ~1 MB max backlog.
const AUDIO_QUEUE_DEPTH: usize = 50;

/// Handle to the running STT pipeline.
///
/// Not Clone — wrap in `Arc` to share across threads.
#[derive(Debug)]
pub struct SttPipeline {
    /// Send raw PCM bytes (f32 LE, 48 kHz mono) into the pipeline.
    audio_tx: SyncSender<Vec<u8>>,
    /// Receive transcribed text from the pipeline.
    /// Wrapped in Mutex so it can be polled from a tokio task.
    pub text_rx: Mutex<Receiver<String>>,
    /// Signals the worker thread to stop.
    shutdown: Arc<AtomicBool>,
    /// Worker thread handle — taken on drop to join cleanly.
    thread: Option<thread::JoinHandle<()>>,
}

impl SttPipeline {
    /// Spawn the pipeline thread.
    ///
    /// Returns `Err` only if the thread cannot be spawned (OS error).
    /// If model files are missing, the worker logs and exits cleanly —
    /// the pipeline handle is still returned but will never produce text.
    pub fn new(model_dir: PathBuf) -> Result<Self, String> {
        let (audio_tx, audio_rx) = mpsc::sync_channel::<Vec<u8>>(AUDIO_QUEUE_DEPTH);
        let (text_tx, text_rx) = mpsc::channel::<String>();
        let shutdown = Arc::new(AtomicBool::new(false));

        let shutdown_worker = Arc::clone(&shutdown);
        let handle = thread::Builder::new()
            .name("stt-worker".into())
            .spawn(move || stt_worker(model_dir, audio_rx, text_tx, shutdown_worker))
            .map_err(|e| format!("failed to spawn stt-worker thread: {e}"))?;

        Ok(Self {
            audio_tx,
            text_rx: Mutex::new(text_rx),
            shutdown,
            thread: Some(handle),
        })
    }

    /// Signal the worker thread to stop.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
    }

    /// Feed raw PCM bytes into the pipeline.
    ///
    /// Non-blocking. Drops audio silently if the pipeline can't keep up —
    /// better to lose frames than to stall the UI thread.
    pub fn push_audio(&self, pcm_bytes: Vec<u8>) -> Result<(), String> {
        // Warn on non-4-byte-aligned input (would silently truncate in bytes_to_f32).
        if pcm_bytes.len() % 4 != 0 {
            eprintln!(
                "sprout-desktop: push_audio_pcm received non-aligned input ({} bytes)",
                pcm_bytes.len()
            );
        }
        // Drop audio if the pipeline can't keep up — better than blocking the UI.
        let _ = self.audio_tx.try_send(pcm_bytes);
        Ok(())
    }
}

impl Drop for SttPipeline {
    fn drop(&mut self) {
        // Signal the worker to stop.
        self.shutdown.store(true, Ordering::Release);
        // Dropping `audio_tx` (implicitly when self is dropped after this fn)
        // unblocks the worker's recv_timeout loop. Join to ensure clean exit.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

// ── Worker thread ─────────────────────────────────────────────────────────────

/// How many 16 kHz samples of silence before we flush to STT.
/// 300 ms × 16 000 Hz / 256 samples-per-frame ≈ 19 frames.
const SILENCE_FLUSH_FRAMES: usize = 19;

/// earshot requires exactly 256 samples per frame at 16 kHz.
const VAD_FRAME_SAMPLES: usize = 256;

/// VAD probability threshold — above this is considered speech.
const VAD_THRESHOLD: f32 = 0.5;

/// How long the worker waits on the audio channel before checking the shutdown flag.
const RECV_TIMEOUT: Duration = Duration::from_millis(50);

fn stt_worker(
    model_dir: PathBuf,
    audio_rx: Receiver<Vec<u8>>,
    text_tx: mpsc::Sender<String>,
    shutdown: Arc<AtomicBool>,
) {
    // ── 1. Initialise rubato resampler (48 kHz → 16 kHz, mono) ───────────────
    use rubato::{Fft, FixedSync, Resampler};

    let mut resampler = match Fft::<f32>::new(48_000, 16_000, 1024, 2, 1, FixedSync::Input) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("sprout-desktop: STT resampler init failed: {e}");
            return;
        }
    };
    let chunk_in = resampler.input_frames_next();

    // ── 2. Initialise earshot VAD ─────────────────────────────────────────────
    use earshot::{DefaultPredictor, Detector};
    let mut vad = Detector::new(DefaultPredictor::new());

    // ── 3. Initialise sherpa-onnx recognizer ─────────────────────────────────
    use sherpa_onnx::{
        OfflineMoonshineModelConfig, OfflineRecognizer, OfflineRecognizerConfig,
    };

    let tokens_path = model_dir.join("tokens.txt");
    if !tokens_path.exists() {
        eprintln!(
            "sprout-desktop: STT models not found at {} — STT disabled",
            model_dir.display()
        );
        // Drain the channel so push_audio doesn't block the sender.
        drain_channel(audio_rx, &shutdown);
        return;
    }

    let model_dir_str = model_dir.to_string_lossy().into_owned();

    let mut cfg = OfflineRecognizerConfig::default();
    cfg.model_config.moonshine = OfflineMoonshineModelConfig {
        preprocessor: Some(format!("{model_dir_str}/preprocessor.onnx")),
        encoder: Some(format!("{model_dir_str}/encoder.onnx")),
        uncached_decoder: None,   // v1 layout only — not used with tiny int8
        cached_decoder: None,     // v1 layout only — not used with tiny int8
        merged_decoder: Some(format!("{model_dir_str}/merged_decoder.onnx")), // v2 (tiny int8)
    };
    cfg.model_config.tokens = Some(tokens_path.to_string_lossy().into_owned());
    cfg.model_config.num_threads = 1;
    cfg.model_config.model_type = Some("moonshine".into());

    let recognizer = match OfflineRecognizer::create(&cfg) {
        Some(r) => r,
        None => {
            eprintln!("sprout-desktop: OfflineRecognizer::create returned None — STT disabled");
            drain_channel(audio_rx, &shutdown);
            return;
        }
    };

    // ── 4. Processing state ───────────────────────────────────────────────────
    // Leftover 48 kHz samples that didn't fill a full resampler chunk.
    let mut input_buf_48k: Vec<f32> = Vec::with_capacity(chunk_in * 2);
    // Leftover 16 kHz samples that didn't fill a full VAD frame.
    let mut leftover_16k: Vec<f32> = Vec::new();
    // Accumulated speech frames (16 kHz).
    let mut speech_buf: Vec<f32> = Vec::new();
    // Consecutive silence frame count.
    let mut silence_frames: usize = 0;
    // Whether we're currently in a speech segment.
    let mut in_speech = false;

    // ── 5. Main loop ──────────────────────────────────────────────────────────
    loop {
        // Check shutdown flag before blocking.
        if shutdown.load(Ordering::Acquire) {
            break;
        }

        // Use recv_timeout so we can periodically check the shutdown flag.
        let bytes = match audio_rx.recv_timeout(RECV_TIMEOUT) {
            Ok(b) => b,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break, // Sender dropped.
        };

        // Drain any additional pending messages to batch-process.
        let mut batch = vec![bytes];
        while let Ok(b) = audio_rx.try_recv() {
            batch.push(b);
        }

        for bytes in batch {
            // Convert raw bytes to f32 samples (little-endian).
            let samples_48k = bytes_to_f32(&bytes);
            input_buf_48k.extend_from_slice(&samples_48k);

            // Resample in chunk_in-sized blocks.
            while input_buf_48k.len() >= chunk_in {
                let chunk: Vec<f32> = input_buf_48k.drain(..chunk_in).collect();
                let resampled = resample_chunk(&mut resampler, &chunk);
                process_16k_samples(
                    &resampled,
                    &mut leftover_16k,
                    &mut vad,
                    &mut speech_buf,
                    &mut silence_frames,
                    &mut in_speech,
                    &recognizer,
                    &text_tx,
                );
            }
        }
    }

    // ── 6. Final flush ────────────────────────────────────────────────────────
    // Transcribe any speech buffered at shutdown so the last utterance isn't lost.
    if !speech_buf.is_empty() {
        flush_to_stt(&speech_buf, &recognizer, &text_tx);
    }
}

/// Resample a mono 48 kHz chunk to 16 kHz using rubato.
/// Returns the resampled samples (may be empty on error).
fn resample_chunk(
    resampler: &mut rubato::Fft<f32>,
    chunk_48k: &[f32],
) -> Vec<f32> {
    use audioadapter_buffers::direct::InterleavedSlice;
    use rubato::Resampler;

    // rubato expects interleaved layout even for mono.
    let input = match InterleavedSlice::new(chunk_48k, 1, chunk_48k.len()) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("sprout-desktop: STT resample input error: {e}");
            return Vec::new();
        }
    };

    match resampler.process(&input, 0, None) {
        Ok(out) => out.take_data(),
        Err(e) => {
            eprintln!("sprout-desktop: STT resample error: {e}");
            Vec::new()
        }
    }
}

/// Feed 16 kHz samples through the VAD and accumulate speech.
/// Flushes to STT when silence exceeds threshold.
fn process_16k_samples(
    samples: &[f32],
    leftover: &mut Vec<f32>,
    vad: &mut earshot::Detector<earshot::DefaultPredictor>,
    speech_buf: &mut Vec<f32>,
    silence_frames: &mut usize,
    in_speech: &mut bool,
    recognizer: &sherpa_onnx::OfflineRecognizer,
    text_tx: &mpsc::Sender<String>,
) {
    leftover.extend_from_slice(samples);

    while leftover.len() >= VAD_FRAME_SAMPLES {
        let frame: Vec<f32> = leftover.drain(..VAD_FRAME_SAMPLES).collect();
        let prob = vad.predict_f32(&frame);
        let is_speech = prob > VAD_THRESHOLD;

        if is_speech {
            *silence_frames = 0;
            *in_speech = true;
            speech_buf.extend_from_slice(&frame);
        } else {
            if *in_speech {
                // Still accumulate during brief silence gaps.
                speech_buf.extend_from_slice(&frame);
                *silence_frames += 1;

                if *silence_frames >= SILENCE_FLUSH_FRAMES {
                    // End of utterance — transcribe.
                    flush_to_stt(speech_buf, recognizer, text_tx);
                    speech_buf.clear();
                    *silence_frames = 0;
                    *in_speech = false;
                }
            }
            // If not in speech, just discard the frame.
        }
    }
}

/// Run sherpa-onnx on the accumulated speech buffer and send the text.
fn flush_to_stt(
    speech_buf: &[f32],
    recognizer: &sherpa_onnx::OfflineRecognizer,
    text_tx: &mpsc::Sender<String>,
) {
    if speech_buf.is_empty() {
        return;
    }

    let stream = recognizer.create_stream();
    stream.accept_waveform(16_000, speech_buf);
    recognizer.decode(&stream);

    let text = stream
        .get_result()
        .map(|r| r.text.trim().to_string())
        .unwrap_or_default();

    if !text.is_empty() {
        if let Err(e) = text_tx.send(text) {
            eprintln!("sprout-desktop: STT text channel closed: {e}");
        }
    }
}

/// Convert raw bytes (f32 LE) to f32 samples.
/// Caller should ensure `bytes.len() % 4 == 0`; extra bytes are silently truncated.
fn bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

/// Drain and discard all pending messages on the channel until shutdown or disconnect.
fn drain_channel(rx: Receiver<Vec<u8>>, shutdown: &AtomicBool) {
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
