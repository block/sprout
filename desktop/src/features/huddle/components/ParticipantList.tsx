import { cn } from "@/shared/lib/cn";

type ParticipantListProps = {
  /** Pubkey hex strings from the Rust huddle state */
  participants: string[];
  className?: string;
};

export function ParticipantList({
  participants,
  className,
}: ParticipantListProps) {
  if (participants.length === 0) return null;

  return (
    <div className={cn("flex items-center gap-1", className)}>
      {participants.map((pubkey) => (
        <ParticipantAvatar key={pubkey} pubkey={pubkey} />
      ))}
    </div>
  );
}

type ParticipantAvatarProps = {
  pubkey: string;
};

function ParticipantAvatar({ pubkey }: ParticipantAvatarProps) {
  // Use first 6 hex chars as a short identifier
  const shortId = pubkey.slice(0, 6).toUpperCase();

  // Derive a stable hue from the pubkey for a distinct avatar color
  const hue = parseInt(pubkey.slice(0, 4), 16) % 360;
  const style = { backgroundColor: `hsl(${hue}, 60%, 55%)`, color: "#fff" };

  return (
    <div
      className={cn(
        "flex h-7 w-7 items-center justify-center rounded-lg text-[9px] font-semibold shadow-sm",
      )}
      style={style}
      title={pubkey}
    >
      {shortId}
    </div>
  );
}
