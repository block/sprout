import { listen, type UnlistenFn } from "@tauri-apps/api/event";

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

/** Return type for setupAudioWorklet — stop + PTT control. */
export type AudioWorkletHandle = {
  stop: () => void;
  /** Send PTT state to the worklet processor. */
  setTransmitting: (active: boolean) => void;
};

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
 *
 * PTT gating:
 *   Main thread listens for Tauri "ptt-state" events (from Rust global shortcut)
 *   and forwards them to the worklet via port.postMessage({ type: 'ptt', active }).
 *   The worklet discards audio frames when transmitting=false.
 *
 * @param audioTrack - Mic track from LiveKit
 * @param initialTransmitting - Initial PTT state. true=open mic (VAD), false=muted until PTT press.
 */
export async function setupAudioWorklet(
  audioTrack: MediaStreamTrack,
  initialTransmitting = true,
): Promise<AudioWorkletHandle> {
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

  // Set initial PTT state (worklet defaults to transmitting=true).
  // In PTT mode, immediately gate audio until the user presses the key.
  if (!initialTransmitting) {
    workletNode.port.postMessage({ type: "ptt", active: false });
  }

  // Forward PCM batches to Rust via raw binary invoke.
  // Direction: worklet→main (receives PCM data from worklet processor).
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

  // Listen for PTT state from Rust global shortcut (Ctrl+Space press/release).
  // Direction: Rust→main→worklet. The Tauri event carries a boolean payload.
  let pttUnlisten: UnlistenFn | null = null;
  try {
    pttUnlisten = await listen<boolean>("ptt-state", (event) => {
      workletNode.port.postMessage({ type: "ptt", active: event.payload });
    });
  } catch {
    // PTT events not available — worklet stays in current transmit mode.
    // This is fine for VAD mode (always transmitting) and degrades gracefully
    // for PTT mode (user won't be able to transmit, but audio won't leak).
  }

  return {
    stop: () => {
      workletNode.port.onmessage = null;
      pttUnlisten?.();
      source.disconnect();
      workletNode.disconnect();
      void audioContext.close();
    },
    setTransmitting: (active: boolean) => {
      workletNode.port.postMessage({ type: "ptt", active });
    },
  };
}
