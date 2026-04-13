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
// NOTE: This mirrors the HuddleState struct in the Rust backend (src-tauri/src/huddle/state.rs).
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
};

type HuddleBarProps = {
  className?: string;
};

export function HuddleBar({ className }: HuddleBarProps) {
  const { localAudioTrack, leaveHuddle, endHuddle, micConnected, micLevel } =
    useHuddle();
  const [state, setState] = React.useState<HuddleState | null>(null);
  const [isMuted, setIsMuted] = React.useState(false);
  // Derive TTS enabled from backend state (single source of truth).
  // Fall back to true if state hasn't loaded yet.
  const ttsEnabled = state?.tts_enabled ?? true;
  const [isLeaving, setIsLeaving] = React.useState(false);
  const [showAddAgent, setShowAddAgent] = React.useState(false);
  const [agentAddError, setAgentAddError] = React.useState<string | null>(null);

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
      await endHuddle();
      setState(null);
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

      {/* Participant avatars */}
      {state.participants.length > 0 && (
        <ParticipantList participants={state.participants} />
      )}

      {/* Voice activity indicator */}
      <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
        {micConnected ? (
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
        ) : (
          <span className="text-destructive/70">no mic</span>
        )}
      </div>

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

      {/* Mute toggle */}
      <Button
        aria-label={isMuted ? "Unmute microphone" : "Mute microphone"}
        aria-pressed={isMuted}
        className="h-8 w-8"
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

      {/* Leave / End button */}
      {state.is_creator ? (
        <Button
          aria-label="End huddle"
          className="h-8 w-8"
          disabled={isLeaving}
          onClick={() => void handleEnd()}
          size="icon"
          variant="destructive"
          title="End huddle for everyone"
        >
          <PhoneOff className="h-4 w-4" />
        </Button>
      ) : (
        <Button
          aria-label="Leave huddle"
          className="h-8 w-8"
          disabled={isLeaving}
          onClick={() => void handleLeave()}
          size="icon"
          variant="destructive"
        >
          <PhoneOff className="h-4 w-4" />
        </Button>
      )}
    </div>
  );
}
