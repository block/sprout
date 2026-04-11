import { invoke } from '@tauri-apps/api/core';
import { Mic, MicOff, PhoneOff, Users } from 'lucide-react';
import * as React from 'react';

import { cn } from '@/shared/lib/cn';
import { Button } from '@/shared/ui/button';
import { ParticipantList } from './ParticipantList';

// Shape returned by the `get_huddle_state` Tauri command
type HuddleState = {
  phase: 'idle' | 'creating' | 'connecting' | 'active' | 'leaving';
  parent_channel_id: string | null;
  ephemeral_channel_id: string | null;
  livekit_token: string | null;
  livekit_url: string | null;
  livekit_room: string | null;
  participants: string[]; // pubkey hex strings
};

type HuddleBarProps = {
  /** MediaStreamTrack for local mic — used for mute toggle */
  localAudioTrack: MediaStreamTrack | null;
  className?: string;
};

export function HuddleBar({ localAudioTrack, className }: HuddleBarProps) {
  const [state, setState] = React.useState<HuddleState | null>(null);
  const [isMuted, setIsMuted] = React.useState(false);
  const [isLeaving, setIsLeaving] = React.useState(false);

  // Poll huddle state — replace with event listener once Rust emits events
  React.useEffect(() => {
    let cancelled = false;

    async function poll() {
      try {
        const s = await invoke<HuddleState>('get_huddle_state');
        if (!cancelled) setState(s);
      } catch {
        // Command not yet registered or no active huddle — ignore
        if (!cancelled) setState(null);
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

  if (!state || state.phase !== 'active') return null;

  async function handleLeave() {
    if (isLeaving) return;
    setIsLeaving(true);
    try {
      await invoke('leave_huddle');
      setState(null);
    } catch (e) {
      console.error('Failed to leave huddle:', e);
    } finally {
      setIsLeaving(false);
    }
  }

  return (
    <div
      className={cn(
        'fixed bottom-4 left-1/2 z-50 -translate-x-1/2',
        'flex items-center gap-3 rounded-xl px-4 py-2',
        'bg-background/95 shadow-lg ring-1 ring-border backdrop-blur-sm',
        className,
      )}
    >
      {/* Room label */}
      <span className="max-w-[120px] truncate text-xs font-medium text-foreground">
        {state.livekit_room ?? 'Huddle'}
      </span>

      {/* Participant count */}
      <div className="flex items-center gap-1 text-xs text-muted-foreground">
        <Users className="h-3 w-3" />
        <span>{state.participants.length}</span>
      </div>

      {/* Participant avatars */}
      {state.participants.length > 0 && (
        <ParticipantList participants={state.participants} />
      )}

      {/* Mute toggle */}
      <Button
        aria-label={isMuted ? 'Unmute microphone' : 'Mute microphone'}
        aria-pressed={isMuted}
        className="h-8 w-8"
        onClick={() => setIsMuted((m) => !m)}
        size="icon"
        variant={isMuted ? 'destructive' : 'secondary'}
      >
        {isMuted ? <MicOff className="h-4 w-4" /> : <Mic className="h-4 w-4" />}
      </Button>

      {/* Leave button */}
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
    </div>
  );
}
