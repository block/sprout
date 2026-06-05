import * as React from "react";

import {
  meshDialEndpointAddr,
  meshStatusReportPayload,
  type MeshCallMeNow,
} from "@/shared/api/tauriMesh";
import type { RelayEvent } from "@/shared/api/types";
import {
  publishMeshStatusReport,
  subscribeToMeshCallMeNow,
} from "@/shared/api/relayMeshSignaling";

const STATUS_REPORT_INTERVAL_MS = 15_000;

function parseCallMeNow(event: RelayEvent): MeshCallMeNow | null {
  try {
    const payload = JSON.parse(event.content) as Partial<MeshCallMeNow>;
    if (
      payload.v !== 1 ||
      payload.type !== "sprout-iroh-call-me-now" ||
      typeof payload.peer_endpoint_addr !== "string" ||
      typeof payload.attempt_id !== "string" ||
      typeof payload.expires_at !== "number"
    ) {
      return null;
    }
    return payload as MeshCallMeNow;
  } catch {
    return null;
  }
}

async function publishCurrentMeshStatus() {
  const payload = await meshStatusReportPayload();
  if (payload == null) return;
  await publishMeshStatusReport(payload);
}

/**
 * Keeps Sprout's relay-owned mesh discovery and punch signaling in sync with the
 * local embedded mesh node. The relay only coordinates tiny control-plane
 * messages; all iroh traffic remains desktop-to-desktop.
 */
export function useMeshRelayOrchestrator(pubkey: string | undefined) {
  React.useEffect(() => {
    if (!pubkey) return;

    let disposed = false;
    let unsubscribeCallMeNow: (() => Promise<void>) | null = null;

    const handleCallMeNow = (event: RelayEvent) => {
      const payload = parseCallMeNow(event);
      if (!payload) return;
      const now = Math.floor(Date.now() / 1_000);
      if (payload.expires_at < now) return;

      void meshDialEndpointAddr(payload.peer_endpoint_addr).catch((error) => {
        console.warn("mesh call-me-now dial failed", {
          attemptId: payload.attempt_id,
          peerEndpointId: payload.peer_endpoint_id,
          error,
        });
      });
    };

    subscribeToMeshCallMeNow(pubkey, handleCallMeNow)
      .then((unsubscribe) => {
        if (disposed) {
          void unsubscribe();
          return;
        }
        unsubscribeCallMeNow = unsubscribe;
      })
      .catch((error) => {
        console.warn("mesh call-me-now subscription failed", error);
      });

    const report = () => {
      void publishCurrentMeshStatus().catch((error) => {
        // Status reports are best-effort; a stale report expires relay-side.
        console.debug("mesh status report skipped", error);
      });
    };
    report();
    const interval = window.setInterval(report, STATUS_REPORT_INTERVAL_MS);

    return () => {
      disposed = true;
      window.clearInterval(interval);
      if (unsubscribeCallMeNow) void unsubscribeCallMeNow();
    };
  }, [pubkey]);
}
