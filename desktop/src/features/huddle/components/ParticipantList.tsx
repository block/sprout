import { cn } from "@/shared/lib/cn";
import { useUsersBatchQuery } from "@/features/profile/hooks";
import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";

type ParticipantListProps = {
  /** Pubkey hex strings from the Rust huddle state */
  participants: string[];
  className?: string;
};

export function ParticipantList({
  participants,
  className,
}: ParticipantListProps) {
  const { data } = useUsersBatchQuery(participants);
  const profiles = data?.profiles ?? {};

  if (participants.length === 0) return null;

  return (
    <div className={cn("flex items-center gap-1", className)}>
      {participants.map((pubkey) => {
        const profile = profiles[pubkey.toLowerCase()];
        const hasProfile = profile?.displayName || profile?.avatarUrl;

        return hasProfile ? (
          <div key={pubkey} title={profile.displayName || pubkey}>
            <ProfileAvatar
              avatarUrl={profile.avatarUrl ?? null}
              label={profile.displayName || pubkey.slice(0, 6)}
              className="h-7 w-7 rounded-lg text-[9px]"
            />
          </div>
        ) : (
          <HexAvatar key={pubkey} pubkey={pubkey} />
        );
      })}
    </div>
  );
}

/** Compact hex-prefix avatar for participants without a loaded profile. */
function HexAvatar({ pubkey }: { pubkey: string }) {
  const shortId = pubkey.slice(0, 6).toUpperCase();
  const parsed = parseInt(pubkey.slice(0, 4), 16);
  const hue = Number.isNaN(parsed) ? 0 : parsed % 360;
  const sat = Number.isNaN(parsed) ? 0 : 60;

  return (
    <div
      className="flex h-7 w-7 items-center justify-center rounded-lg text-[9px] font-semibold shadow-sm"
      style={{ backgroundColor: `hsl(${hue}, ${sat}%, 55%)`, color: "#fff" }}
      title={pubkey}
    >
      {shortId}
    </div>
  );
}
