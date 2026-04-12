// AudioWorklet processor — runs in the AudioWorklet thread.
// Accumulates PCM Float32 samples and sends 100ms batches to the main thread.
//
// Note: when the worklet is disconnected, any partial buffer (< 4800 samples)
// is silently dropped. This means the last ~100ms of speech may be lost on
// huddle leave. This is acceptable — the STT pipeline's silence-flush threshold
// (450ms) means the last utterance was already transcribed before disconnect.
class SttTapProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.buffer = new Float32Array(4800); // ~100ms at 48kHz
    this.offset = 0;
  }

  process(inputs) {
    const input = inputs[0]?.[0]; // mono channel
    if (!input) return true;

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
