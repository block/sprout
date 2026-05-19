import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { Cpu } from "lucide-react";

import { Switch } from "@/shared/ui/switch";
import { Input } from "@/shared/ui/input";

// ---------------------------------------------------------------------------
// Types matching the Rust mesh_llm::ComputeSharingPrefs / ResourceCaps shape.
// ---------------------------------------------------------------------------

interface ResourceCaps {
  max_vram_mb: number | null;
  max_ram_mb: number | null;
  max_concurrency: number | null;
}

interface ModelOffer {
  id: string;
  label?: string | null;
  context_tokens?: number | null;
}

interface ComputeSharingPrefs {
  enabled: boolean;
  caps: ResourceCaps;
  models: ModelOffer[];
  d_tag: string;
}

interface MeshEndpointInfo {
  endpoint_id: string;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a string into Some(n) when it represents a positive integer, or
/// None to clear the cap. Empty string also clears.
function parseCap(raw: string): number | null {
  if (raw.trim() === "") return null;
  const n = Number.parseInt(raw, 10);
  if (Number.isFinite(n) && n >= 0) return n;
  return null;
}

function formatCap(value: number | null): string {
  return value == null ? "" : String(value);
}

// ---------------------------------------------------------------------------
// Card
// ---------------------------------------------------------------------------

export function MeshComputeSettingsCard() {
  const [prefs, setPrefs] = React.useState<ComputeSharingPrefs | null>(null);
  const [endpoint, setEndpoint] = React.useState<MeshEndpointInfo | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);

  // Load the persisted prefs + the iroh endpoint identity on mount.
  React.useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [p, e] = await Promise.all([
          invoke<ComputeSharingPrefs>("mesh_get_sharing_prefs"),
          invoke<MeshEndpointInfo>("mesh_get_endpoint_id"),
        ]);
        if (!cancelled) {
          setPrefs(p);
          setEndpoint(e);
        }
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const persist = React.useCallback(async (next: ComputeSharingPrefs) => {
    setSaving(true);
    setError(null);
    try {
      await invoke("mesh_set_sharing_prefs", { prefs: next });
      setPrefs(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }, []);

  if (!prefs) {
    return (
      <section className="min-w-0" data-testid="settings-compute">
        <div className="mb-3">
          <h2 className="text-sm font-semibold tracking-tight">
            Share compute
          </h2>
          <p className="text-sm text-muted-foreground">
            {error ?? "Loading mesh-LLM preferences…"}
          </p>
        </div>
      </section>
    );
  }

  const updateCap = (field: keyof ResourceCaps, raw: string) => {
    persist({
      ...prefs,
      caps: { ...prefs.caps, [field]: parseCap(raw) },
    });
  };

  return (
    <section className="min-w-0" data-testid="settings-compute">
      <div className="mb-3 min-w-0">
        <h2 className="text-sm font-semibold tracking-tight">Share compute</h2>
        <p className="text-sm text-muted-foreground">
          When enabled, other members of this relay can run agents on this
          machine using the limits you set below. Your relay membership is the
          only gate — there is no signup or external account.
        </p>
      </div>

      {error ? (
        <p className="mb-3 rounded bg-destructive/10 px-2 py-1 text-xs text-destructive">
          {error}
        </p>
      ) : null}

      <div className="flex flex-col gap-4">
        {/* ── Master toggle ────────────────────────────────────────── */}
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <label
              className="text-sm font-medium"
              htmlFor="mesh-compute-enabled"
            >
              Share this machine's compute
            </label>
            <p className="text-sm text-muted-foreground">
              Publishes a kind:31990 compute-offer event when on; deletes it
              when off.
            </p>
          </div>
          <Switch
            checked={prefs.enabled}
            data-testid="mesh-compute-enabled-toggle"
            disabled={saving}
            id="mesh-compute-enabled"
            onCheckedChange={(checked) =>
              persist({ ...prefs, enabled: checked })
            }
          />
        </div>

        {/* ── Caps ─────────────────────────────────────────────────── */}
        <fieldset className="flex flex-col gap-3" disabled={!prefs.enabled}>
          <legend className="flex items-center gap-2 text-sm font-medium">
            <Cpu className="h-4 w-4 text-muted-foreground" />
            Limits per request
          </legend>

          <div className="grid grid-cols-3 gap-3">
            <div>
              <label
                className="text-xs text-muted-foreground"
                htmlFor="mesh-vram"
              >
                Max VRAM (MB)
              </label>
              <Input
                data-testid="mesh-cap-vram"
                id="mesh-vram"
                inputMode="numeric"
                onChange={(e) => updateCap("max_vram_mb", e.target.value)}
                placeholder="No limit"
                value={formatCap(prefs.caps.max_vram_mb)}
              />
            </div>
            <div>
              <label
                className="text-xs text-muted-foreground"
                htmlFor="mesh-ram"
              >
                Max RAM (MB)
              </label>
              <Input
                data-testid="mesh-cap-ram"
                id="mesh-ram"
                inputMode="numeric"
                onChange={(e) => updateCap("max_ram_mb", e.target.value)}
                placeholder="No limit"
                value={formatCap(prefs.caps.max_ram_mb)}
              />
            </div>
            <div>
              <label
                className="text-xs text-muted-foreground"
                htmlFor="mesh-concurrency"
              >
                Concurrent peers
              </label>
              <Input
                data-testid="mesh-cap-concurrency"
                id="mesh-concurrency"
                inputMode="numeric"
                onChange={(e) => updateCap("max_concurrency", e.target.value)}
                placeholder="1"
                value={formatCap(prefs.caps.max_concurrency)}
              />
            </div>
          </div>
        </fieldset>

        {/* ── Identity ────────────────────────────────────────────── */}
        {endpoint ? (
          <div className="rounded border border-border/60 bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
            <span className="font-medium text-foreground">
              This device's iroh endpoint:
            </span>{" "}
            <code className="break-all">{endpoint.endpoint_id}</code>
          </div>
        ) : null}
      </div>
    </section>
  );
}
