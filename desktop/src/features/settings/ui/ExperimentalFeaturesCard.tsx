import { desktopFeatures, useFeatureToggle, useDevToggle } from "@/shared/features";
import type { FeatureDefinition } from "@/shared/features";

function FeatureRow({ feature }: { feature: FeatureDefinition }) {
  const [enabled, toggle] = useFeatureToggle(feature.id);

  return (
    <label className="flex items-center justify-between gap-3 rounded-lg border border-border/70 bg-background/70 px-4 py-3">
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium">{feature.name}</p>
        <p className="text-xs text-muted-foreground">{feature.description}</p>
      </div>
      <input
        checked={enabled}
        className="h-4 w-4 accent-primary"
        onChange={(e) => toggle(e.target.checked)}
        type="checkbox"
      />
    </label>
  );
}

export function ExperimentalFeaturesCard() {
  const [devEnabled, setDevEnabled] = useDevToggle();
  const isDev = import.meta.env.DEV;

  const experimentalFeatures = desktopFeatures.filter(
    (f) => f.tier === "experimental",
  );
  const devFeatures = desktopFeatures.filter((f) => f.tier === "dev");

  return (
    <section className="min-w-0" data-testid="settings-experimental">
      <div className="mb-3 min-w-0">
        <h2 className="text-sm font-semibold tracking-tight">
          Experimental Features
        </h2>
        <p className="text-sm text-muted-foreground">
          These features are functional but still being refined. Enable them to
          try new capabilities early.
        </p>
      </div>

      <div className="flex flex-col gap-2">
        {experimentalFeatures.map((f) => (
          <FeatureRow feature={f} key={f.id} />
        ))}
      </div>

      {isDev && (
        <>
          <div className="mb-3 mt-6 min-w-0">
            <h2 className="text-sm font-semibold tracking-tight">
              Developer Features
            </h2>
            <p className="text-sm text-muted-foreground">
              Only visible in development builds. Toggle the master switch to
              hide all dev features.
            </p>
          </div>

          <label className="mb-3 flex items-center justify-between gap-3 rounded-lg border border-primary/30 bg-primary/5 px-4 py-3">
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium">Show developer features</p>
              <p className="text-xs text-muted-foreground">
                When off, all dev-tier features are hidden
              </p>
            </div>
            <input
              checked={devEnabled}
              className="h-4 w-4 accent-primary"
              onChange={(e) => setDevEnabled(e.target.checked)}
              type="checkbox"
            />
          </label>

          {devEnabled && (
            <div className="flex flex-col gap-2">
              {devFeatures.map((f) => (
                <FeatureRow feature={f} key={f.id} />
              ))}
            </div>
          )}
        </>
      )}
    </section>
  );
}
