import { ExternalLink, Users } from "lucide-react";
import { useMemo } from "react";

import { relayWsUrl } from "@/shared/lib/relay-url";
import { Button } from "@/shared/ui/button";
import type { Repo } from "../use-repos";

const MAX_AVATARS = 20;

/** Simple hash of a hex pubkey to a hue value (0-360). */
function pubkeyToHue(hex: string): number {
  let hash = 0;
  for (let i = 0; i < hex.length; i++) {
    hash = (hash * 31 + hex.charCodeAt(i)) | 0;
  }
  return Math.abs(hash) % 360;
}

function PubkeyAvatar({ pubkey }: { pubkey: string }) {
  const hue = pubkeyToHue(pubkey);
  return (
    <div
      className="flex h-8 w-8 items-center justify-center rounded-full text-xs font-medium text-white"
      style={{ backgroundColor: `hsl(${hue}, 55%, 45%)` }}
      title={pubkey}
    >
      {pubkey.slice(0, 2)}
    </div>
  );
}

export function OrgSidebar({ repos }: { repos: Repo[] }) {
  const deepLink = `sprout://connect?relay=${encodeURIComponent(relayWsUrl())}`;

  const uniquePubkeys = useMemo(() => {
    const set = new Set<string>();
    for (const repo of repos) {
      set.add(repo.owner);
      for (const c of repo.contributors) {
        set.add(c);
      }
    }
    return [...set];
  }, [repos]);

  const visiblePubkeys = uniquePubkeys.slice(0, MAX_AVATARS);
  const overflowCount = uniquePubkeys.length - MAX_AVATARS;

  return (
    <div className="space-y-6">
      {/* Connect to Relay */}
      <Button asChild className="w-full">
        <a href={deepLink}>
          <ExternalLink className="h-4 w-4" />
          Connect to Relay
        </a>
      </Button>

      {/* People section */}
      {uniquePubkeys.length > 0 && (
        <div>
          <h3 className="mb-3 flex items-center gap-2 text-sm font-semibold text-sidebar-foreground">
            <Users className="h-4 w-4" />
            People
          </h3>
          <div className="flex flex-wrap gap-2">
            {visiblePubkeys.map((pk) => (
              <PubkeyAvatar key={pk} pubkey={pk} />
            ))}
          </div>
          {overflowCount > 0 && (
            <button
              type="button"
              className="mt-2 text-xs text-muted-foreground hover:text-foreground"
            >
              View all {uniquePubkeys.length} people
            </button>
          )}
        </div>
      )}
    </div>
  );
}
