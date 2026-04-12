/**
 * Raw binary invoke — uses Tauri's internal IPC for zero-copy ArrayBuffer transfer.
 *
 * The typed @tauri-apps/api doesn't support raw binary payloads (InvokeBody::Raw).
 * This wrapper isolates the internal API dependency to a single call site.
 * Tested against Tauri v2. If this breaks on upgrade, only this function needs updating.
 */
function invokeRawBinary(cmd: string, payload: Uint8Array): Promise<unknown> {
  // biome-ignore lint/suspicious/noExplicitAny: Tauri internals have no public type definition
  const internals = (window as any).__TAURI_INTERNALS__;
  if (!internals?.invoke) {
    return Promise.reject(new Error("Tauri internals not available"));
  }
  return internals.invoke(cmd, payload);
}

/**
 * AudioWorklet → Rust STT pipeline:
 *
 *   MediaStreamTrack (mic, 48kHz)
 *     → AudioContext.createMediaStreamSource()
 *     → AudioWorkletNode("stt-tap-processor")
 *         worklet.js accumulates 100ms batches (4800 samples)
 *         posts Float32Array to main thread via port.postMessage
 *     → onmessage: convert to Uint8Array view (zero-copy)
 *     → invokeRawBinary("push_audio_pcm", bytes)
 *         Rust: SttPipeline::push_audio → bounded sync_channel
 */

export async function setupAudioWorklet(
  audioTrack: MediaStreamTrack,
): Promise<{ stop: () => void }> {
  const audioContext = new AudioContext({ sampleRate: 48000 });

  // Resume after user gesture (required by autoplay policy)
  if (audioContext.state === "suspended") {
    await audioContext.resume();
  }

  // Load the worklet processor (must live in public/ for Vite to serve it)
  await audioContext.audioWorklet.addModule("/worklet.js");

  // Create source from the mic track
  const source = audioContext.createMediaStreamSource(
    new MediaStream([audioTrack]),
  );

  // Create worklet node
  const workletNode = new AudioWorkletNode(audioContext, "stt-tap-processor");

  // Connect: mic → worklet (tap only — no playback)
  source.connect(workletNode);

  // Forward PCM batches to Rust via raw binary invoke
  workletNode.port.onmessage = (event: MessageEvent<Float32Array>) => {
    const float32 = event.data;
    // Fire-and-forget — Rust side uses try_send which drops on backpressure.
    // No await: prevents main-thread backpressure from slow Rust processing.
    // Create a zero-copy Uint8Array view over the same underlying buffer.
    // Rust reinterprets the bytes as f32 on the other side.
    invokeRawBinary(
      "push_audio_pcm",
      new Uint8Array(float32.buffer, float32.byteOffset, float32.byteLength),
    ).catch(() => {
      /* silently drop — Rust handles backpressure */
    });
  };

  return {
    stop: () => {
      workletNode.port.onmessage = null;
      source.disconnect();
      workletNode.disconnect();
      void audioContext.close();
    },
  };
}
