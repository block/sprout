import { invoke } from "@tauri-apps/api/core";
import {
  Mic,
  MicOff,
  PhoneOff,
  Plus,
  Users,
  Volume2,
  VolumeX,
} from "lucide-react";
import * as React from "react";

import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { useHuddle } from "../HuddleContext";
import { AddAgentDialog, type AgentAddResult } from "./AddAgentDialog";
import { ParticipantList } from "./ParticipantList";

// Shape returned by the `get_huddle_state` Tauri command.
// NOTE: This mirrors the HuddleState struct in the Rust backend (src-tauri/src/huddle/mod.rs).
// If you add/remove fields here, update the Rust struct (and vice versa).
type HuddleState = {
  phase:
    | "idle"
    | "creating"
    | "connecting"
    | "connected"
    | "active"
    | "leaving";
  parent_channel_id: string | null;
  ephemeral_channel_id: string | null;
  livekit_room: string | null;
  participants: string[]; // pubkey hex strings
  agent_pubkeys: string[];
  tts_enabled: boolean;
  is_creator: boolean;
  voice_input_mode: "push_to_talk" | "voice_activity";
};

type HuddleBarProps = {
  className?: string;
};

export function HuddleBar({ className }: HuddleBarProps) {
  const {
    localAudioTrack,
    leaveHuddle,
    endHuddle,
    micConnected,
    micLevel,
    pttActive,
    voiceInputMode,
    setVoiceInputMode,
    activeSpeakers,
    isReconnecting,
  } = useHuddle();

  const isPttMode = voiceInputMode === "push_to_talk";
  const [state, setState] = React.useState<HuddleState | null>(null);
  const [isMuted, setIsMuted] = React.useState(false);
  // Derive TTS enabled from backend state (single source of truth).
  // Fall back to true if state hasn't loaded yet.
  const ttsEnabled = state?.tts_enabled ?? true;
  const [isLeaving, setIsLeaving] = React.useState(false);
  const [showAddAgent, setShowAddAgent] = React.useState(false);
  const [agentAddError, setAgentAddError] = React.useState<string | null>(null);
  const [modelStatus, setModelStatus] = React.useState<{
    moonshine: string;
    kokoro: string;
  } | null>(null);

  // Poll huddle state — replace with event listener once Rust emits events
  React.useEffect(() => {
    let cancelled = false;

    async function poll() {
      try {
        const s = await invoke<HuddleState>("get_huddle_state");
        if (!cancelled) setState(s);
      } catch {
        // Only clear state if we never had an active huddle.
        // Transient errors shouldn't remove the control bar.
        if (!cancelled) {
          setState((prev) =>
            prev?.phase === "active" || prev?.phase === "connected"
              ? prev
              : null,
          );
        }
      }
    }

    void poll();
    const id = window.setInterval(() => void poll(), 2_000);

    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, []);

  // Poll model download status while huddle is active
  const huddlePhase = state?.phase;
  React.useEffect(() => {
    if (huddlePhase !== "active" && huddlePhase !== "connected") return;

    let cancelled = false;

    // ModelStatus serializes as: "ready" | "not_downloaded" (strings)
    // or { downloading: { progress_percent: N } } | { error: "msg" } (objects).
    const fmt = (s: unknown): string => {
      if (typeof s === "string") return s === "ready" ? "ready" : "pending";
      if (typeof s === "object" && s !== null) {
        if ("downloading" in s) {
          const d = (s as { downloading: { progress_percent: number } })
            .downloading;
          return `${d.progress_percent}%`;
        }
        if ("error" in s) return "error";
      }
      return "pending";
    };

    async function pollModels() {
      try {
        const status = await invoke<{
          moonshine: unknown;
          kokoro: unknown;
        }>("get_model_status");
        if (cancelled) return;

        setModelStatus({
          moonshine: fmt(status.moonshine),
          kokoro: fmt(status.kokoro),
        });
      } catch {
        // best-effort
      }
    }

    void pollModels();
    const id = window.setInterval(() => void pollModels(), 3_000);

    return () => {
      cancelled = true;
      window.clearInterval(id);
      setModelStatus(null); // Clear stale status on huddle end/phase change.
    };
  }, [huddlePhase]);

  // Sync mute state to the audio track
  React.useEffect(() => {
    if (localAudioTrack) {
      localAudioTrack.enabled = !isMuted;
    }
  }, [isMuted, localAudioTrack]);

  if (!state || (state.phase !== "active" && state.phase !== "connected"))
    return null;

  async function handleLeave() {
    if (isLeaving) return;
    setIsLeaving(true);
    try {
      const backendClean = await leaveHuddle();
      if (backendClean) {
        setState(null);
      }
      // If backend cleanup failed, keep the bar visible so the user can retry.
      // leaveHuddle retains rustActiveRef=true for the next attempt.
    } catch (e) {
      console.error("Failed to leave huddle:", e);
    } finally {
      setIsLeaving(false);
    }
  }

  async function handleEnd() {
    if (isLeaving) return;
    setIsLeaving(true);
    try {
      const backendClean = await endHuddle();
      if (backendClean) {
        setState(null);
      }
      // If backend cleanup failed, keep the bar visible so the user can retry.
    } catch (e) {
      console.error("Failed to end huddle:", e);
    } finally {
      setIsLeaving(false);
    }
  }

  return (
    <div
      className={cn(
        "fixed bottom-4 left-1/2 z-50 -translate-x-1/2",
        "flex items-center gap-3 rounded-xl px-4 py-2",
        "bg-background/95 shadow-lg ring-1 ring-border backdrop-blur-sm",
        className,
      )}
    >
      {/* Room label */}
      <span className="text-xs font-medium text-foreground">Huddle</span>

      {/* Huddle status */}
      <div className="flex items-center gap-1 text-xs text-muted-foreground">
        <Users className="h-3 w-3" />
        <span>In huddle</span>
      </div>

      {/* Reconnecting indicator */}
      {isReconnecting && (
        <div className="flex items-center gap-1 text-xs text-amber-500">
          <span className="animate-pulse">Reconnecting…</span>
        </div>
      )}

      {/* Model download progress */}
      {modelStatus &&
        (modelStatus.moonshine !== "ready" ||
          modelStatus.kokoro !== "ready") && (
          <output className="flex items-center gap-1 text-xs text-muted-foreground">
            <span className="animate-pulse">
              {modelStatus.moonshine !== "ready" &&
              modelStatus.kokoro !== "ready"
                ? `Voice models: STT ${modelStatus.moonshine}, TTS ${modelStatus.kokoro}`
                : modelStatus.moonshine !== "ready"
                  ? `STT model: ${modelStatus.moonshine}`
                  : `TTS model: ${modelStatus.kokoro}`}
            </span>
          </output>
        )}

      {/* Participant avatars */}
      {state.participants.length > 0 && (
        <ParticipantList
          participants={state.participants}
          activeSpeakers={activeSpeakers}
        />
      )}

      {/* Voice input mode indicator */}
      <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
        {micConnected ? (
          isPttMode ? (
            <>
              <div
                className={cn(
                  "h-2.5 w-2.5 rounded-full transition-colors",
                  pttActive && !isMuted
                    ? "bg-green-500 animate-pulse"
                    : "bg-zinc-500",
                )}
                title={
                  isMuted
                    ? "Muted (PTT overridden)"
                    : pttActive
                      ? "Transmitting"
                      : "Push Ctrl+Space to talk"
                }
              />
              <span>PTT</span>
              <span className="text-[10px] opacity-60">Ctrl+Space</span>
            </>
          ) : (
            <>
              <div
                className="h-2.5 w-2.5 rounded-full transition-colors"
                style={{
                  backgroundColor:
                    micLevel > 0.05
                      ? `rgba(34, 197, 94, ${0.4 + micLevel * 0.6})`
                      : "rgba(100, 116, 139, 0.4)",
                }}
                title={`Mic level: ${Math.round(micLevel * 100)}%`}
              />
              <span>VAD</span>
            </>
          )
        ) : (
          <span className="text-destructive/70">no mic</span>
        )}
      </div>

      {/* Voice input mode toggle */}
      <Button
        aria-label={
          isPttMode
            ? "Switch to voice activity mode"
            : "Switch to push-to-talk mode"
        }
        className="h-6 px-1.5 text-[10px]"
        onClick={() =>
          void setVoiceInputMode(isPttMode ? "voice_activity" : "push_to_talk")
        }
        size="sm"
        variant="ghost"
        title={
          isPttMode ? "Switch to Voice Activity" : "Switch to Push-to-Talk"
        }
      >
        {isPttMode ? "→ VAD" : "→ PTT"}
      </Button>

      {/* Add agent button */}
      <Button
        aria-label="Add agent to huddle"
        className="h-8 w-8"
        onClick={() => setShowAddAgent(true)}
        size="icon"
        variant="secondary"
      >
        <Plus className="h-4 w-4" />
      </Button>

      {agentAddError && (
        <span className="max-w-[180px] truncate rounded bg-destructive/10 px-2 py-1 text-xs text-destructive">
          {agentAddError}
        </span>
      )}

      {showAddAgent && (
        <AddAgentDialog
          currentAgentPubkeys={state?.agent_pubkeys ?? []}
          onClose={() => setShowAddAgent(false)}
          onAdd={async (pubkey: string): Promise<AgentAddResult> => {
            setAgentAddError(null);
            try {
              return await invoke<AgentAddResult>("add_agent_to_huddle", {
                agentPubkey: pubkey,
              });
            } catch (e: unknown) {
              const msg = e instanceof Error ? e.message : String(e);
              setAgentAddError(`Failed to add agent: ${msg}`);
              throw e; // Re-throw so AddAgentDialog shows its inline error.
            }
          }}
        />
      )}

      {/* Mute toggle — in PTT mode acts as hard mute override (even PTT won't transmit) */}
      <Button
        aria-label={
          isMuted
            ? "Unmute microphone"
            : isPttMode
              ? "Force mute (overrides PTT)"
              : "Mute microphone"
        }
        aria-pressed={isMuted}
        className={cn(
          "h-8 w-8",
          isPttMode &&
            pttActive &&
            !isMuted &&
            "ring-2 ring-green-500 ring-offset-1 ring-offset-background",
        )}
        onClick={() => setIsMuted((m) => !m)}
        size="icon"
        variant={isMuted ? "destructive" : "secondary"}
      >
        {isMuted ? <MicOff className="h-4 w-4" /> : <Mic className="h-4 w-4" />}
      </Button>

      {/* TTS toggle */}
      <Button
        aria-label={ttsEnabled ? "Mute agent speech" : "Unmute agent speech"}
        aria-pressed={!ttsEnabled}
        className="h-8 w-8"
        onClick={async () => {
          const next = !ttsEnabled;
          try {
            await invoke("set_tts_enabled", { enabled: next });
            // Refresh state immediately so the UI reflects the change
            const s = await invoke<HuddleState>("get_huddle_state");
            setState(s);
          } catch (e) {
            console.error("Failed to toggle TTS:", e);
          }
        }}
        size="icon"
        variant={ttsEnabled ? "secondary" : "destructive"}
      >
        {ttsEnabled ? (
          <Volume2 className="h-4 w-4" />
        ) : (
          <VolumeX className="h-4 w-4" />
        )}
      </Button>

      {/* Leave / End buttons — available to all participants */}
      <Button
        aria-label="Leave huddle"
        className="h-8 w-8"
        disabled={isLeaving}
        onClick={() => void handleLeave()}
        size="icon"
        variant="destructive"
        title="Leave huddle"
      >
        <PhoneOff className="h-4 w-4" />
      </Button>

      {state?.is_creator && (
        <Button
          aria-label="End huddle for everyone"
          className="h-6 px-1.5 text-[10px]"
          disabled={isLeaving}
          onClick={() => void handleEnd()}
          size="sm"
          variant="ghost"
          title="End huddle for everyone (archives channel)"
        >
          End all
        </Button>
      )}

      {/* Screen reader announcements for huddle state changes */}
      <output aria-live="polite" className="sr-only">
        {isReconnecting
          ? "Huddle reconnecting"
          : micConnected
            ? "In huddle, microphone connected"
            : "In huddle, no microphone"}
        {modelStatus &&
          modelStatus.moonshine !== "ready" &&
          `, STT model ${modelStatus.moonshine}`}
        {modelStatus &&
          modelStatus.kokoro !== "ready" &&
          `, TTS model ${modelStatus.kokoro}`}
      </output>
    </div>
  );
}
