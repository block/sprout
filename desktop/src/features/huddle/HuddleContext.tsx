import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import * as React from "react";

import { relayClient } from "@/shared/api/relayClient";
import { setupAudioWorklet, type AudioWorkletHandle } from "./lib/audioWorklet";

/**
 * Huddle lifecycle (React context):
 *   startHuddle/joinHuddle → invoke(start/join_huddle) → getUserMedia + setupAudioWorklet
 *     → confirm_huddle_active
 *   TTS subscription: subscribeToChannelLive → filter agent pubkeys → speak_agent_message
 *   leaveHuddle: stop worklet → stop mic track → invoke(leave_huddle)
 *   Active speakers: Tauri "huddle-active-speakers" event (Rust backend emits)
 */

type HuddleJoinInfo = {
  ephemeral_channel_id: string;
};

type VoiceInputMode = "push_to_talk" | "voice_activity";

interface HuddleContextValue {
  /** Current local audio track (for mute toggle in HuddleBar) */
  localAudioTrack: MediaStreamTrack | null;
  /** Whether a huddle is being started (for button disabled state) */
  isStarting: boolean;
  /** Last start/join error message — display in UI and clear with clearHuddleError */
  huddleError: string | null;
  /** Dismiss the current huddleError */
  clearHuddleError: () => void;
  /** Whether the mic connection is live */
  micConnected: boolean;
  /** Current mic input level 0–1 (updated via requestAnimationFrame) */
  micLevel: number;
  /** Whether the PTT key is currently held (for UI feedback) */
  pttActive: boolean;
  /** Current voice input mode — push_to_talk or voice_activity */
  voiceInputMode: VoiceInputMode;
  /** Toggle voice input mode (persisted to Rust backend) */
  setVoiceInputMode: (mode: VoiceInputMode) => Promise<void>;
  /** Pubkeys of currently speaking participants (from Rust backend) */
  activeSpeakers: string[];
  /** Start a new huddle — calls Rust start_huddle, then connects mic + AudioWorklet */
  startHuddle: (
    parentChannelId: string,
    memberPubkeys: string[],
  ) => Promise<void>;
  /** Join an existing huddle — calls Rust join_huddle, then connects mic + AudioWorklet */
  joinHuddle: (
    parentChannelId: string,
    ephemeralChannelId: string,
  ) => Promise<void>;
  /** Leave the current huddle — stops worklet, stops mic, calls Rust leave_huddle.
   *  Returns true if backend cleanup succeeded, false if it failed (caller may retry). */
  leaveHuddle: () => Promise<boolean>;
  /** End the huddle (creator only) — archives ephemeral channel, emits huddle_ended */
  endHuddle: () => Promise<boolean>;
}

const HuddleContext = React.createContext<HuddleContextValue | null>(null);

export function HuddleProvider({ children }: { children: React.ReactNode }) {
  const workletRef = React.useRef<AudioWorkletHandle | null>(null);
  const tokenRef = React.useRef(0);
  const busyRef = React.useRef(false);
  /** True once Rust `start_huddle` or `join_huddle` has been invoked (even if JS-side refs aren't populated yet). */
  const rustActiveRef = React.useRef(false);
  const [localAudioTrack, setLocalAudioTrack] =
    React.useState<MediaStreamTrack | null>(null);
  const [isStarting, setIsStarting] = React.useState(false);
  const [huddleError, setHuddleError] = React.useState<string | null>(null);
  const clearHuddleError = React.useCallback(() => setHuddleError(null), []);
  const [micConnected, setMicConnected] = React.useState(false);
  const [micLevel, setMicLevel] = React.useState(0);
  /** Whether the PTT key is currently held */
  const [pttActive, setPttActive] = React.useState(false);
  /** Current voice input mode */
  const [voiceInputMode, setVoiceInputModeState] =
    React.useState<VoiceInputMode>("push_to_talk");
  /** Ref tracking latest voiceInputMode — read inside connectAndSetupMedia to
   *  avoid stale closure capture when the user toggles mode mid-start. */
  const voiceInputModeRef = React.useRef<VoiceInputMode>("push_to_talk");
  voiceInputModeRef.current = voiceInputMode;
  /** Ephemeral channel ID — set after start_huddle/join_huddle, used for TTS subscription */
  const [ephemeralChannelId, setEphemeralChannelId] = React.useState<
    string | null
  >(null);
  /** Self pubkey — fetched once, used to filter out own messages from TTS */
  const selfPubkeyRef = React.useRef<string | null>(null);
  /** Pubkeys of participants currently speaking (from Rust backend via Tauri event) */
  const [activeSpeakers, setActiveSpeakers] = React.useState<string[]>([]);

  // Bootstrap voice input mode from Rust backend on mount.
  // Ensures frontend stays in sync after remount/recovery.
  React.useEffect(() => {
    invoke<VoiceInputMode>("get_voice_input_mode")
      .then((mode) => setVoiceInputModeState(mode))
      .catch(() => {
        /* best-effort — default is push_to_talk */
      });
  }, []);

  // Active speakers from Rust backend (emitted by the audio relay recv task).
  React.useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    listen<string[]>("huddle-active-speakers", (event) => {
      if (!cancelled) setActiveSpeakers(event.payload);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // Persistent AudioContext for PTT audio cues — reused across all PTT presses
  // to avoid exhausting the browser's ~6 concurrent AudioContext limit.
  const pttAudioCtxRef = React.useRef<AudioContext | null>(null);

  // PTT state from Rust (Ctrl+Space). UI feedback + 50ms audio cue when mic active.
  // Actual audio gating is in audioWorklet.ts → worklet.js.
  React.useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    listen<boolean>("ptt-state", (event) => {
      if (cancelled) return;
      setPttActive(event.payload);
      if (micConnected) {
        try {
          if (
            !pttAudioCtxRef.current ||
            pttAudioCtxRef.current.state === "closed"
          ) {
            pttAudioCtxRef.current = new AudioContext();
          }
          const ac = pttAudioCtxRef.current;
          const osc = ac.createOscillator();
          const g = ac.createGain();
          osc.connect(g);
          g.connect(ac.destination);
          osc.frequency.value = event.payload ? 880 : 440;
          g.gain.value = 0.05;
          osc.start();
          osc.stop(ac.currentTime + 0.05);
        } catch {
          /* best-effort */
        }
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
      // Close the PTT AudioContext when the effect is cleaned up.
      if (pttAudioCtxRef.current && pttAudioCtxRef.current.state !== "closed") {
        void pttAudioCtxRef.current.close();
        pttAudioCtxRef.current = null;
      }
    };
  }, [micConnected]);

  // Toggle voice input mode — persists to Rust backend and updates worklet gating.
  const setVoiceInputMode = React.useCallback(async (mode: VoiceInputMode) => {
    await invoke("set_voice_input_mode", { mode });
    setVoiceInputModeState(mode);
    workletRef.current?.setMode(mode);
  }, []);

  // Ref-track the current audio track so disconnectMedia is stable (no
  // dependency on localAudioTrack state). This prevents the unmount-cleanup
  // effect from re-firing mid-startup when setLocalAudioTrack triggers a
  // leaveHuddle dependency chain update.
  const audioTrackRef = React.useRef<MediaStreamTrack | null>(null);
  audioTrackRef.current = localAudioTrack;

  /** Stop AudioWorklet and mic track. Best-effort on all steps. */
  const disconnectMedia = React.useCallback(async () => {
    // Invalidate any in-flight startHuddle/joinHuddle
    tokenRef.current += 1;
    try {
      workletRef.current?.stop();
    } catch {
      /* best-effort */
    }
    workletRef.current = null;
    audioTrackRef.current?.stop();
    setLocalAudioTrack(null);
    setMicConnected(false);
    setEphemeralChannelId(null);
    setActiveSpeakers([]);
  }, []); // Stable — reads track from ref, not state.

  const leaveHuddle = React.useCallback(async (): Promise<boolean> => {
    await disconnectMedia();
    if (rustActiveRef.current) {
      try {
        await invoke("leave_huddle");
        rustActiveRef.current = false;
      } catch {
        // Leave rustActiveRef true so a subsequent leaveHuddle() retries Rust cleanup
        return false; // Signal that backend cleanup failed
      }
    }
    return true; // Backend cleanup succeeded (or was not needed)
  }, [disconnectMedia]);

  const endHuddle = React.useCallback(async (): Promise<boolean> => {
    await disconnectMedia();
    if (rustActiveRef.current) {
      try {
        await invoke("end_huddle");
        rustActiveRef.current = false;
        return true;
      } catch {
        // end_huddle failed — fall back to local leave so we at least
        // disconnect, but report false so the UI knows the huddle was
        // NOT ended for everyone (no archive, no huddle_ended event).
        try {
          await invoke("leave_huddle");
          rustActiveRef.current = false;
        } catch {
          // Leave rustActiveRef true so a subsequent call retries
        }
        return false;
      }
    }
    return true;
  }, [disconnectMedia]);

  /**
   * Clean up a partially-established huddle. Best-effort on every step.
   *
   * Takes explicit worklet/stream args (not from refs) because startHuddle/joinHuddle
   * may have local variables that differ from the refs mid-flight.
   */
  const cleanupFailedStart = React.useCallback(
    async (
      worklet: AudioWorkletHandle | null,
      stream: MediaStream | null,
      isCreator: boolean,
    ) => {
      try {
        worklet?.stop();
      } catch {
        /* best-effort */
      }
      if (stream)
        stream.getTracks().forEach((t) => {
          t.stop();
        });
      setLocalAudioTrack(null);
      setMicConnected(false);
      setEphemeralChannelId(null);
      setActiveSpeakers([]);
      if (rustActiveRef.current) {
        if (isCreator) {
          try {
            await invoke("end_huddle");
            rustActiveRef.current = false;
          } catch {
            try {
              await invoke("leave_huddle");
              rustActiveRef.current = false;
            } catch {}
          }
        } else {
          try {
            await invoke("leave_huddle");
            rustActiveRef.current = false;
          } catch {}
        }
      }
    },
    [],
  );

  /** Shared media setup: get mic, setup AudioWorklet, confirm active.
   *  Used by both startHuddle and joinHuddle after the Rust backend call succeeds. */
  const connectAndSetupMedia = React.useCallback(
    async (
      joinInfo: HuddleJoinInfo,
      myToken: number,
    ): Promise<{
      worklet: AudioWorkletHandle;
      stream: MediaStream;
    }> => {
      // Fetch self pubkey once for TTS filtering
      if (!selfPubkeyRef.current) {
        try {
          const identity = await invoke<{ pubkey: string }>("get_identity");
          selfPubkeyRef.current = identity.pubkey;
        } catch {
          /* best-effort */
        }
      }

      if (tokenRef.current !== myToken) throw new Error("superseded");

      // Get mic — Rust backend owns the audio WS connection
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: { echoCancellation: true, noiseSuppression: true },
      });
      const audioTrack = stream.getAudioTracks()[0];

      // Wrap post-getUserMedia steps so the stream is always cleaned up on
      // failure — prevents the mic permission light staying on after errors.
      try {
        if (tokenRef.current !== myToken) {
          throw new Error("superseded");
        }

        setLocalAudioTrack(audioTrack);
        setMicConnected(true);

        // Setup AudioWorklet — PCM goes to Rust via push_audio_pcm
        const initialTransmitting =
          voiceInputModeRef.current !== "push_to_talk";
        const worklet = await setupAudioWorklet(
          audioTrack,
          initialTransmitting,
        );

        if (tokenRef.current !== myToken) {
          worklet.stop();
          throw new Error("superseded");
        }

        workletRef.current = worklet;
        setEphemeralChannelId(joinInfo.ephemeral_channel_id);
        await invoke("confirm_huddle_active");

        return { worklet, stream };
      } catch (err) {
        // Always stop the mic stream on any failure path.
        stream.getTracks().forEach((t) => {
          t.stop();
        });
        setLocalAudioTrack(null);
        setMicConnected(false);
        throw err;
      }
    },
    [],
  );

  const startHuddle = React.useCallback(
    async (parentChannelId: string, memberPubkeys: string[]) => {
      if (busyRef.current) return;
      busyRef.current = true;

      tokenRef.current += 1;
      const myToken = tokenRef.current;

      setIsStarting(true);
      try {
        // Step 1: Call Rust to create ephemeral channel
        const joinInfo = await invoke<HuddleJoinInfo>("start_huddle", {
          parentChannelId,
          memberPubkeys,
        });
        rustActiveRef.current = true;
        // Step 2–4: Get mic, setup AudioWorklet, confirm active
        try {
          await connectAndSetupMedia(joinInfo, myToken);
        } catch (e) {
          if (e instanceof Error && e.message === "superseded") {
            await cleanupFailedStart(workletRef.current, null, true);
            return;
          }
          throw e;
        }
      } catch (e) {
        const w = workletRef.current;
        workletRef.current = null;
        await cleanupFailedStart(w, null, true);
        const msg = e instanceof Error ? e.message : String(e);
        setHuddleError(msg);
        console.error("Failed to start huddle:", e);
        throw e;
      } finally {
        setIsStarting(false);
        busyRef.current = false;
      }
    },
    [cleanupFailedStart, connectAndSetupMedia],
  );

  const joinHuddle = React.useCallback(
    async (parentChannelId: string, ephemeralChannelId: string) => {
      if (busyRef.current) return;
      busyRef.current = true;
      tokenRef.current += 1;
      const myToken = tokenRef.current;
      setIsStarting(true);

      try {
        // Step 1: Call Rust join_huddle
        const joinInfo = await invoke<HuddleJoinInfo>("join_huddle", {
          parentChannelId,
          ephemeralChannelId,
        });
        rustActiveRef.current = true;

        // Step 2–4: Get mic, setup AudioWorklet, confirm active
        try {
          await connectAndSetupMedia(joinInfo, myToken);
        } catch (e) {
          if (e instanceof Error && e.message === "superseded") {
            await cleanupFailedStart(workletRef.current, null, false);
            return;
          }
          throw e;
        }
      } catch (e) {
        const w = workletRef.current;
        workletRef.current = null;
        await cleanupFailedStart(w, null, false);
        const msg = e instanceof Error ? e.message : String(e);
        setHuddleError(msg);
        console.error("Failed to join huddle:", e);
        throw e;
      } finally {
        setIsStarting(false);
        busyRef.current = false;
      }
    },
    [cleanupFailedStart, connectAndSetupMedia],
  );

  // TTS subscription — pipe AGENT messages from ephemeral channel to speak_agent_message.
  // Human STT transcripts are also kind:9 in this channel, so we must filter them out
  // using an authoritative agent list fetched from the relay membership API.
  React.useEffect(() => {
    if (!ephemeralChannelId) return;

    let disposed = false;
    let cleanup: (() => void) | null = null;

    // ── Agent identity (authoritative, fail-closed) ───────────────────────
    //
    // Fetch the ephemeral channel's member list from the relay REST API and
    // identify agents by their "bot" role. This is authoritative — it works
    // for both creators and joiners, and reflects mid-huddle agent additions.
    //
    // FAIL-CLOSED: agentsLoaded starts false. Until the fetch succeeds and
    // populates agentPubkeys, NO messages are spoken. An empty set after a
    // successful fetch means "no agents in the huddle" → still mute.
    let agentsLoaded = false;
    const agentPubkeys = new Set<string>();

    async function loadAgentPubkeys() {
      try {
        const pubkeys = await invoke<string[]>("get_huddle_agent_pubkeys");
        agentPubkeys.clear();
        for (const pk of pubkeys) agentPubkeys.add(pk);
        agentsLoaded = true;
      } catch (e) {
        // Fail-closed on ALL failures, including refresh after prior success.
        // Clear the set and mark as not loaded — TTS goes mute until the
        // next successful refresh. Stale membership must never authorize speech.
        agentPubkeys.clear();
        agentsLoaded = false;
        console.error("[huddle] Failed to load agent pubkeys:", e);
      }
    }

    // Initial load + periodic refresh (catches mid-huddle agent additions).
    void loadAgentPubkeys();
    const agentRefreshId = window.setInterval(() => {
      void loadAgentPubkeys();
    }, 10_000);

    // ── Live-only subscription ───────────────────────────────────────────
    // subscribeToChannelLive uses `since: now` — the relay never sends
    // historical backlog. Every event delivered is a live message.
    // Event-ID dedup handles reconnect replay (same event arriving twice).
    const seenEventIds = new Set<string>();
    const seenOrder: string[] = [];
    const MAX_SEEN_EVENTS = 5000;

    relayClient
      .subscribeToChannelLive(ephemeralChannelId, (event) => {
        if (disposed) return;
        // Defense-in-depth: subscription already filters to kind:9 only.
        if (event.kind !== 9) return;

        // Dedup by event ID (covers reconnect replay).
        if (seenEventIds.has(event.id)) return;
        seenEventIds.add(event.id);
        seenOrder.push(event.id);
        if (seenOrder.length > MAX_SEEN_EVENTS) {
          const oldest = seenOrder.shift();
          if (oldest !== undefined) seenEventIds.delete(oldest);
        }

        // Fail-closed: don't speak until agent list is loaded.
        if (!agentsLoaded) return;
        // Only speak agent messages — skip human STT transcripts.
        if (!agentPubkeys.has(event.pubkey)) return;
        if (event.pubkey === selfPubkeyRef.current) return;
        if (event.content.trim().length <= 1) return;
        // Legacy: skip [System]-prefixed messages from before kind:48106.
        if (event.content.startsWith("[System]")) return;
        invoke("speak_agent_message", { text: event.content }).catch((err) => {
          console.warn(
            "[huddle] TTS speak failed (backpressure or pipeline unavailable):",
            err,
          );
        });
      })
      .then((dispose) => {
        if (disposed) {
          void dispose();
          return;
        }
        cleanup = () => void dispose();
      })
      .catch((err) => {
        console.error("[huddle] TTS subscription failed:", err);
      });

    return () => {
      disposed = true;
      cleanup?.();
      window.clearInterval(agentRefreshId);
    };
  }, [ephemeralChannelId]);

  // Pipeline hot-start — check if voice models finished downloading mid-huddle
  React.useEffect(() => {
    if (!ephemeralChannelId) return;
    const id = window.setInterval(() => {
      invoke("check_pipeline_hotstart").catch(() => {
        /* best-effort */
      });
    }, 5_000);
    return () => window.clearInterval(id);
  }, [ephemeralChannelId]);

  // Mic level analyser — drives the voice activity indicator
  React.useEffect(() => {
    if (!localAudioTrack) {
      setMicLevel(0);
      return;
    }

    const ctx = new AudioContext();
    const analyser = ctx.createAnalyser();
    analyser.fftSize = 256;
    const source = ctx.createMediaStreamSource(
      new MediaStream([localAudioTrack]),
    );
    source.connect(analyser);
    const buf = new Uint8Array(analyser.frequencyBinCount);

    let raf = 0;
    let lastUpdate = 0;
    function tick(now: number) {
      raf = requestAnimationFrame(tick);
      // Throttle state updates to ~10fps — voice meters don't need 60fps
      // visual fidelity, and setMicLevel re-renders the entire HuddleBar.
      if (now - lastUpdate < 100) return;
      lastUpdate = now;
      analyser.getByteFrequencyData(buf);
      // RMS-ish: average of frequency bins, normalized to 0–1
      let sum = 0;
      for (let i = 0; i < buf.length; i++) sum += buf[i];
      setMicLevel(sum / (buf.length * 255));
    }
    raf = requestAnimationFrame(tick);

    return () => {
      cancelAnimationFrame(raf);
      source.disconnect();
      void ctx.close();
    };
  }, [localAudioTrack]);

  // Cleanup on unmount only — stable ref prevents re-firing mid-startup.
  const leaveHuddleRef = React.useRef(leaveHuddle);
  leaveHuddleRef.current = leaveHuddle;
  React.useEffect(() => {
    return () => {
      void leaveHuddleRef.current();
    };
  }, []);

  return (
    <HuddleContext.Provider
      value={{
        localAudioTrack,
        isStarting,
        huddleError,
        clearHuddleError,
        micConnected,
        micLevel,
        pttActive,
        voiceInputMode,
        setVoiceInputMode,
        activeSpeakers,
        startHuddle,
        joinHuddle,
        leaveHuddle,
        endHuddle,
      }}
    >
      {children}
    </HuddleContext.Provider>
  );
}

export function useHuddle(): HuddleContextValue {
  const ctx = React.useContext(HuddleContext);
  if (!ctx) {
    throw new Error("useHuddle must be used within a HuddleProvider");
  }
  return ctx;
}
