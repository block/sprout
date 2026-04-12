import { invoke } from "@tauri-apps/api/core";
import * as React from "react";

import { relayClient } from "@/shared/api/relayClient";
import { connectToHuddle, type HuddleConnection } from "./lib/livekit";
import { setupAudioWorklet } from "./lib/audioWorklet";

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
  /** Leave the current huddle — disconnects LiveKit, stops worklet, calls Rust leave_huddle */
  leaveHuddle: () => Promise<void>;
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
  const analyserRef = React.useRef<{
    ctx: AudioContext;
    analyser: AnalyserNode;
    raf: number;
  } | null>(null);

  const leaveHuddle = React.useCallback(async () => {
    // Invalidate any in-flight startHuddle so it bails after its next await
    tokenRef.current += 1;

    // Step 1: Stop AudioWorklet (best-effort — don't let a throw skip remaining cleanup)
    try {
      workletRef.current?.stop();
    } catch {
      /* best-effort */
    }
    workletRef.current = null;

    // Step 2: Disconnect LiveKit (best-effort, null ref first to prevent double-disconnect)
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

    // Step 3: Tell Rust to clean up — only clear rustActiveRef AFTER success so retries work
    if (rustActiveRef.current) {
      try {
        await invoke("leave_huddle");
        rustActiveRef.current = false;
      } catch {
        // Leave rustActiveRef true so a subsequent leaveHuddle() retries Rust cleanup
      }
    }
  }, []);

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
        setEphemeralChannelId(joinInfo.ephemeral_channel_id);

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
          try {
            await invoke("leave_huddle");
            rustActiveRef.current = false;
          } catch {
            /* leave rustActiveRef true so leaveHuddle retries */
          }
          return;
        }

        // Step 2: Connect to LiveKit room
        connection = await connectToHuddle(
          joinInfo.livekit_url,
          joinInfo.livekit_token,
        );

        // Bail if superseded after async connect
        if (tokenRef.current !== myToken) {
          try {
            await connection.disconnect();
          } catch {
            /* best-effort */
          }
          try {
            await invoke("leave_huddle");
            rustActiveRef.current = false;
          } catch {
            /* leave rustActiveRef true so leaveHuddle retries */
          }
          return;
        }

        connectionRef.current = connection;
        setLocalAudioTrack(connection.localAudioTrack);
        setMicConnected(true);

        // Step 3: Set up AudioWorklet to pipe mic audio to Rust STT
        const worklet = await setupAudioWorklet(connection.localAudioTrack);

        // Bail if superseded after async worklet setup
        if (tokenRef.current !== myToken) {
          try {
            worklet.stop();
          } catch {
            /* best-effort */
          }
          try {
            await connection.disconnect();
          } catch {
            /* best-effort */
          }
          connectionRef.current = null;
          setLocalAudioTrack(null);
          try {
            await invoke("leave_huddle");
            rustActiveRef.current = false;
          } catch {
            /* leave rustActiveRef true so leaveHuddle retries */
          }
          return;
        }

        workletRef.current = worklet;
      } catch (e) {
        // Clean up the LOCAL connection captured above, not whatever is in the ref
        try {
          if (connection) await connection.disconnect();
        } catch {
          /* best-effort */
        }
        connectionRef.current = null;
        setLocalAudioTrack(null);
        // Tell Rust to reset from Creating/Active back to Idle and archive orphaned channel
        if (rustActiveRef.current) {
          try {
            await invoke("leave_huddle");
            rustActiveRef.current = false;
          } catch {
            /* leave rustActiveRef true so leaveHuddle retries */
          }
        }
        console.error("Failed to start huddle:", e);
        throw e;
      } finally {
        setIsStarting(false);
        busyRef.current = false;
      }
    },
    [],
  );

  // TTS subscription — pipe agent messages from ephemeral channel to speak_agent_message
  React.useEffect(() => {
    if (!ephemeralChannelId) return;

    let disposed = false;
    let cleanup: (() => void) | null = null;
    const seenIds = new Set<string>();

    relayClient
      .subscribeToChannel(ephemeralChannelId, (event) => {
        if (disposed) return;
        // Only kind:9 (chat messages), skip own messages and duplicates
        if (event.kind !== 9) return;
        if (seenIds.has(event.id)) return;
        seenIds.add(event.id);
        if (event.pubkey === selfPubkeyRef.current) return;
        // Skip [System] messages
        if (event.content.startsWith("[System]")) return;

        invoke("speak_agent_message", { text: event.content }).catch(() => {
          /* best-effort */
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
    };
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

    analyserRef.current = { ctx, analyser, raf };

    return () => {
      cancelAnimationFrame(raf);
      source.disconnect();
      void ctx.close();
      analyserRef.current = null;
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
