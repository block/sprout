// AudioWorklet processor — runs in the AudioWorklet thread.
// Accumulates PCM Float32 samples and sends 100ms batches to the main thread.
//
// Supports push-to-talk (PTT) gating: when `this.transmitting` is false,
// incoming audio frames are discarded and the buffer is reset. The main thread
// sends `{ type: 'ptt', active: boolean }` messages to toggle transmission.
// Default: transmitting=true (open mic for VAD mode compatibility).
//
// Note: when the worklet is disconnected, any partial buffer (< 4800 samples)
// is silently dropped. The last ~100ms of speech may be lost on huddle leave.
// This is acceptable for voice — losing a partial syllable at disconnect is
// imperceptible compared to the natural end-of-conversation flow.
class SttTapProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.buffer = new Float32Array(4800); // ~100ms at 48kHz
    this.offset = 0;
    this.transmitting = true; // default: open (VAD mode). PTT mode sets false on init.

    // Listen for PTT state changes from main thread.
    // Direction: main→worklet (receives). The worklet→main direction uses
    // this.port.postMessage for PCM data — these don't conflict.
    this.port.onmessage = (e) => {
      if (e.data && e.data.type === "ptt") {
        this.transmitting = e.data.active;
      }
    };
  }

  process(inputs) {
    const input = inputs[0]?.[0]; // mono channel
    if (!input) return true;

    // PTT gating: discard frames when not transmitting.
    // Reset buffer offset so we don't send stale audio when PTT activates.
    if (!this.transmitting) {
      this.offset = 0;
      return true;
    }

    // Accumulate samples
    const remaining = this.buffer.length - this.offset;
    const toCopy = Math.min(input.length, remaining);
    this.buffer.set(input.subarray(0, toCopy), this.offset);
    this.offset += toCopy;

    // Flush when buffer is full
    if (this.offset >= this.buffer.length) {
      // Transfer ownership for zero-copy
      this.port.postMessage(this.buffer, [this.buffer.buffer]);
      this.buffer = new Float32Array(4800);
      this.offset = 0;

      // Handle leftover samples
      if (toCopy < input.length) {
        const leftover = input.subarray(toCopy);
        this.buffer.set(leftover);
        this.offset = leftover.length;
      }
    }

    return true;
  }
}

registerProcessor("stt-tap-processor", SttTapProcessor);
