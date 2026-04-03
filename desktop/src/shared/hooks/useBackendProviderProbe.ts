import * as React from "react";

import { probeBackendProvider } from "@/shared/api/tauri";
import type {
  BackendProviderCandidate,
  BackendProviderProbeResult,
} from "@/shared/api/types";

type UseBackendProviderProbeResult = {
  probedProvider: BackendProviderProbeResult | null;
  probeError: string | null;
  providerConfig: Record<string, string>;
  setProviderConfig: React.Dispatch<
    React.SetStateAction<Record<string, string>>
  >;
};

/**
 * Probes a backend provider binary and extracts config schema defaults.
 * Fires whenever `isProviderMode` or `selectedBackendProvider` changes.
 */
export function useBackendProviderProbe(
  isProviderMode: boolean,
  selectedBackendProvider: BackendProviderCandidate | null | undefined,
): UseBackendProviderProbeResult {
  const [probedProvider, setProbedProvider] =
    React.useState<BackendProviderProbeResult | null>(null);
  const [probeError, setProbeError] = React.useState<string | null>(null);
  const [providerConfig, setProviderConfig] = React.useState<
    Record<string, string>
  >({});

  React.useEffect(() => {
    if (!isProviderMode || !selectedBackendProvider) {
      setProbedProvider(null);
      setProbeError(null);
      return;
    }

    let cancelled = false;
    setProbeError(null);
    setProbedProvider(null);

    probeBackendProvider(selectedBackendProvider.binaryPath)
      .then((result) => {
        if (!cancelled) {
          setProbedProvider(result);
          // Seed config from schema defaults so unchanged defaults are
          // included in the submit payload rather than silently dropped.
          if (result.config_schema) {
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            const props = (result.config_schema as any)?.properties ?? {};
            const defaults: Record<string, string> = {};
            for (const [key, prop] of Object.entries(props) as [
              string,
              Record<string, unknown>,
            ][]) {
              if (prop.default != null) {
                defaults[key] = String(prop.default);
              }
            }
            setProviderConfig(defaults);
          }
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setProbeError(err instanceof Error ? err.message : String(err));
        }
      });

    return () => {
      cancelled = true;
    };
  }, [isProviderMode, selectedBackendProvider]);

  return { probedProvider, probeError, providerConfig, setProviderConfig };
}
