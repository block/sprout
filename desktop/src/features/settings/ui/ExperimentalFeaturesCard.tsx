import { useState } from "react";
import { desktopFeatures, useFeatureToggle } from "@/shared/features";
import type { FeatureDefinition } from "@/shared/features";
import { Switch } from "@/shared/ui/switch";
import { Badge } from "@/shared/ui/badge";
import { ChevronDown, ChevronRight } from "lucide-react";

function FeatureRow({
  feature,
  showBadge,
}: {
  feature: FeatureDefinition;
  showBadge?: boolean;
}) {
  const [enabled, toggle] = useFeatureToggle(feature.id);

  return (
    <label className="flex items-center justify-between gap-3 rounded-lg border border-border/70 bg-background/70 px-4 py-3">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <p className="text-sm font-medium">{feature.name}</p>
          {showBadge && <Badge variant="warning">unstable</Badge>}
        </div>
        <p className="text-xs text-muted-foreground">{feature.description}</p>
      </div>
      <Switch
        checked={enabled}
        data-testid={`feature-toggle-${feature.id}`}
        onCheckedChange={toggle}
      />
    </label>
  );
}

export function ExperimentalFeaturesCard() {
  const [showUnstable, setShowUnstable] = useState(false);

  const previewFeatures = desktopFeatures.filter(
    (f) => f.tier === "preview",
  );
  const unstableFeatures = desktopFeatures.filter(
    (f) => f.tier === "unstable",
  );

  return (
    <section className="min-w-0" data-testid="settings-experimental">
      <div className="mb-3 min-w-0">
        <p className="text-sm text-muted-foreground">
          These features are functional but still being refined. Enable them to
          try new capabilities early.
        </p>
      </div>

      <div className="flex flex-col gap-2">
        {previewFeatures.map((f) => (
          <FeatureRow feature={f} key={f.id} />
        ))}
      </div>

      {unstableFeatures.length > 0 && (
        <>
          <button
            className="mt-4 flex w-full items-center gap-2 rounded-lg border border-border/50 bg-muted/30 px-4 py-2.5 text-left text-sm text-muted-foreground transition-colors hover:bg-muted/50"
            onClick={() => setShowUnstable(!showUnstable)}
            type="button"
          >
            {showUnstable ? (
              <ChevronDown className="h-4 w-4" />
            ) : (
              <ChevronRight className="h-4 w-4" />
            )}
            <span>Show unstable features</span>
            <Badge className="ml-auto" variant="warning">
              {unstableFeatures.length}
            </Badge>
          </button>

          {showUnstable && (
            <div className="mt-2 flex flex-col gap-2">
              {unstableFeatures.map((f) => (
                <FeatureRow feature={f} key={f.id} showBadge />
              ))}
            </div>
          )}
        </>
      )}
    </section>
  );
}
