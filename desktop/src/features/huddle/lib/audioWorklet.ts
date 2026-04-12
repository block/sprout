// Tauri internals surface — not part of the public @tauri-apps/api but
// available at runtime in the webview. We use it here for raw binary invoke
// (InvokeBody::Raw on the Rust side) which the typed wrapper doesn't support.
declare global {
  interface Window {
    __TAURI_INTERNALS__: {
      invoke: (cmd: string, payload?: unknown) => Promise<unknown>;
    };
  }
}

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
    // Tauri v2 InvokeBody::Raw only accepts ArrayBuffer | Uint8Array.
    // Create a zero-copy Uint8Array view over the same underlying buffer.
    // Rust reinterprets the bytes as f32 on the other side.
    window.__TAURI_INTERNALS__
      .invoke(
        "push_audio_pcm",
        new Uint8Array(float32.buffer, float32.byteOffset, float32.byteLength),
      )
      .catch(() => {
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
