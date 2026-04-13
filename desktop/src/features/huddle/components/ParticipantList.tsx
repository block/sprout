import { cn } from "@/shared/lib/cn";
import { useUsersBatchQuery } from "@/features/profile/hooks";
import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";

type ParticipantListProps = {
  /** Pubkey hex strings from the Rust huddle state */
  participants: string[];
  activeSpeakers?: string[];
  className?: string;
};

export function ParticipantList({
  participants,
  activeSpeakers,
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
        const isActive = activeSpeakers?.includes(pubkey);
        const ariaLabel =
          profile?.displayName || `Participant ${pubkey.slice(0, 8)}`;

        return hasProfile ? (
          <div
            key={pubkey}
            aria-label={ariaLabel}
            role="img"
            title={profile.displayName || pubkey}
          >
            <ProfileAvatar
              avatarUrl={profile.avatarUrl ?? null}
              label={profile.displayName || pubkey.slice(0, 6)}
              className={cn(
                "h-7 w-7 rounded-lg text-[9px]",
                isActive &&
                  "ring-2 ring-green-500 ring-offset-1 ring-offset-background",
              )}
            />
          </div>
        ) : (
          <HexAvatar
            key={pubkey}
            pubkey={pubkey}
            activeSpeakers={activeSpeakers}
          />
        );
      })}
    </div>
  );
}

/** Compact hex-prefix avatar for participants without a loaded profile. */
function HexAvatar({
  pubkey,
  activeSpeakers,
}: {
  pubkey: string;
  activeSpeakers?: string[];
}) {
  const shortId = pubkey.slice(0, 6).toUpperCase();
  const parsed = parseInt(pubkey.slice(0, 4), 16);
  const hue = Number.isNaN(parsed) ? 0 : parsed % 360;
  const sat = Number.isNaN(parsed) ? 0 : 60;
  const isActive = activeSpeakers?.includes(pubkey);

  return (
    <div
      aria-label={`Participant ${pubkey.slice(0, 8)}`}
      role="img"
      className={cn(
        "flex h-7 w-7 items-center justify-center rounded-lg text-[9px] font-semibold shadow-sm",
        isActive &&
          "ring-2 ring-green-500 ring-offset-1 ring-offset-background",
      )}
      style={{ backgroundColor: `hsl(${hue}, ${sat}%, 55%)`, color: "#fff" }}
      title={pubkey}
    >
      {shortId}
    </div>
  );
}
