import { invoke } from "@tauri-apps/api/core";
import * as React from "react";

import { relayClient } from "@/shared/api/relayClient";
import { connectToHuddle, type HuddleConnection } from "./lib/livekit";
import { setupAudioWorklet } from "./lib/audioWorklet";

/**
 * Huddle lifecycle (React context):
 *
 *   startHuddle(channelId, agents)
 *     → invoke("start_huddle")         [Rust: ephemeral channel + LiveKit token]
 *     → connectToHuddle(url, token)     [LiveKit: WebRTC room + mic]
 *     → setupAudioWorklet(track)        [AudioWorklet: mic PCM → Rust STT]
 *     → invoke("confirm_huddle_active") [Rust: Connected → Active]
 *     → setEphemeralChannelId(...)      [triggers TTS subscription + hotstart polling]
 *
 *   TTS subscription (on ephemeralChannelId change):
 *     → relayClient.subscribeToChannelLive(ephId, callback)
 *     → live-only (since: now) — no historical backlog
 *     → filter: agent pubkeys only (fail-closed), skip self
 *     → invoke("speak_agent_message", { text })
 *
 *   leaveHuddle()
 *     → stop AudioWorklet → disconnect LiveKit → invoke("leave_huddle")
 */

type HuddleJoinInfo = {
  ephemeral_channel_id: string;
  livekit_token: string;
  livekit_url: string;
  livekit_room: string;
};

interface HuddleContextValue {
  /** Current local audio track (for mute toggle in HuddleBar) */
  localAudioTrack: MediaStreamTrack | null;
  /** Whether a huddle is being started (for button disabled state) */
  isStarting: boolean;
  /** Whether the LiveKit + mic connection is live */
  micConnected: boolean;
  /** Current mic input level 0–1 (updated via requestAnimationFrame) */
  micLevel: number;
  /** Start a new huddle — calls Rust start_huddle, then connects LiveKit + AudioWorklet */
  startHuddle: (
    parentChannelId: string,
    memberPubkeys: string[],
  ) => Promise<void>;
  /** Leave the current huddle — disconnects LiveKit, stops worklet, calls Rust leave_huddle.
   *  Returns true if backend cleanup succeeded, false if it failed (caller may retry). */
  leaveHuddle: () => Promise<boolean>;
  /** End the huddle (creator only) — archives ephemeral channel, emits huddle_ended */
  endHuddle: () => Promise<boolean>;
}

const HuddleContext = React.createContext<HuddleContextValue | null>(null);

export function HuddleProvider({ children }: { children: React.ReactNode }) {
  const connectionRef = React.useRef<HuddleConnection | null>(null);
  const workletRef = React.useRef<{ stop: () => void } | null>(null);
  const tokenRef = React.useRef(0);
  const busyRef = React.useRef(false);
  /** True once Rust `start_huddle` has been invoked (even if JS-side refs aren't populated yet). */
  const rustActiveRef = React.useRef(false);
  const [localAudioTrack, setLocalAudioTrack] =
    React.useState<MediaStreamTrack | null>(null);
  const [isStarting, setIsStarting] = React.useState(false);
  const [micConnected, setMicConnected] = React.useState(false);
  const [micLevel, setMicLevel] = React.useState(0);
  /** Ephemeral channel ID — set after start_huddle, used for TTS subscription */
  const [ephemeralChannelId, setEphemeralChannelId] = React.useState<
    string | null
  >(null);
  /** Self pubkey — fetched once, used to filter out own messages from TTS */
  const selfPubkeyRef = React.useRef<string | null>(null);

  /** Stop AudioWorklet and disconnect LiveKit. Best-effort on both steps. */
  const disconnectMedia = React.useCallback(async () => {
    // Invalidate any in-flight startHuddle
    tokenRef.current += 1;

    // Step 1: Stop AudioWorklet
    try {
      workletRef.current?.stop();
    } catch {
      /* best-effort */
    }
    workletRef.current = null;

    // Step 2: Disconnect LiveKit (null ref first to prevent double-disconnect)
    const conn = connectionRef.current;
    connectionRef.current = null;
    try {
      if (conn) await conn.disconnect();
    } catch {
      /* best-effort */
    }
    setLocalAudioTrack(null);
    setMicConnected(false);
    setEphemeralChannelId(null);
  }, []);

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
   * Note: takes explicit conn/worklet args (not from refs) because startHuddle
   * may have local variables that differ from the refs mid-flight. Can't use
   * disconnectMedia() here for the same reason.
   */
  const cleanupFailedStart = React.useCallback(
    async (
      conn: HuddleConnection | null,
      worklet: { stop: () => void } | null,
    ) => {
      try {
        worklet?.stop();
      } catch {
        /* best-effort */
      }
      try {
        if (conn) await conn.disconnect();
      } catch {
        /* best-effort */
      }
      connectionRef.current = null;
      setLocalAudioTrack(null);
      setMicConnected(false);
      setEphemeralChannelId(null);
      // Use end_huddle (not leave_huddle) for creator cleanup —
      // this archives the ephemeral channel and emits huddle_ended,
      // preventing orphaned huddles visible to other users.
      if (rustActiveRef.current) {
        try {
          await invoke("end_huddle");
          rustActiveRef.current = false;
        } catch {
          // Fall back to leave_huddle if end_huddle fails
          // (e.g. non-creator, or end_huddle not available)
          try {
            await invoke("leave_huddle");
            rustActiveRef.current = false;
          } catch {
            // Leave rustActiveRef true so a subsequent call retries
          }
        }
      }
    },
    [],
  );

  const startHuddle = React.useCallback(
    async (parentChannelId: string, memberPubkeys: string[]) => {
      // Synchronous concurrency guard — belt-and-suspenders alongside isStarting state
      if (busyRef.current) return;
      busyRef.current = true;

      tokenRef.current += 1;
      const myToken = tokenRef.current;

      setIsStarting(true);
      let connection: HuddleConnection | null = null;
      try {
        // Step 1: Call Rust to create ephemeral channel + get LiveKit token
        const joinInfo = await invoke<HuddleJoinInfo>("start_huddle", {
          parentChannelId,
          memberPubkeys,
        });
        rustActiveRef.current = true;
        // Do NOT set ephemeralChannelId yet — wait until fully established (LiveKit + Worklet)

        // Fetch self pubkey once for TTS filtering
        if (!selfPubkeyRef.current) {
          try {
            const identity = await invoke<{ pubkey: string }>("get_identity");
            selfPubkeyRef.current = identity.pubkey;
          } catch {
            /* best-effort — TTS will just speak all messages */
          }
        }

        // Bail if superseded (leaveHuddle or another startHuddle was called)
        if (tokenRef.current !== myToken) {
          await cleanupFailedStart(null, null);
          return;
        }

        // Step 2: Connect to LiveKit room
        connection = await connectToHuddle(
          joinInfo.livekit_url,
          joinInfo.livekit_token,
        );

        // Bail if superseded after async connect
        if (tokenRef.current !== myToken) {
          await cleanupFailedStart(connection, null);
          return;
        }

        connectionRef.current = connection;
        setLocalAudioTrack(connection.localAudioTrack);
        setMicConnected(true);

        // Step 3: Set up AudioWorklet to pipe mic audio to Rust STT
        const worklet = await setupAudioWorklet(connection.localAudioTrack);

        // Bail if superseded after async worklet setup
        if (tokenRef.current !== myToken) {
          await cleanupFailedStart(connection, worklet);
          return;
        }

        workletRef.current = worklet;
        // Step 4: Huddle fully established — now safe to set ephemeralChannelId
        // This triggers TTS subscription and hot-start polling effects
        setEphemeralChannelId(joinInfo.ephemeral_channel_id);

        // Confirm to backend that media is established — transitions Connected → Active.
        await invoke("confirm_huddle_active");
      } catch (e) {
        // Pass workletRef.current — it may have been assigned before the error
        // (e.g. confirm_huddle_active rejects after worklet setup succeeded).
        const w = workletRef.current;
        workletRef.current = null;
        await cleanupFailedStart(connection, w);
        console.error("Failed to start huddle:", e);
        throw e;
      } finally {
        setIsStarting(false);
        busyRef.current = false;
      }
    },
    [cleanupFailedStart],
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
    function tick() {
      analyser.getByteFrequencyData(buf);
      // RMS-ish: average of frequency bins, normalized to 0–1
      let sum = 0;
      for (let i = 0; i < buf.length; i++) sum += buf[i];
      setMicLevel(sum / (buf.length * 255));
      raf = requestAnimationFrame(tick);
    }
    raf = requestAnimationFrame(tick);

    return () => {
      cancelAnimationFrame(raf);
      source.disconnect();
      void ctx.close();
    };
  }, [localAudioTrack]);

  // Cleanup on unmount — fire and forget
  React.useEffect(() => {
    return () => {
      void leaveHuddle();
    };
  }, [leaveHuddle]);

  return (
    <HuddleContext.Provider
      value={{
        localAudioTrack,
        isStarting,
        micConnected,
        micLevel,
        startHuddle,
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
