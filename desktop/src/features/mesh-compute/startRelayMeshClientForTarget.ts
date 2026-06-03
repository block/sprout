import { publishMeshConnectRequest } from "@/shared/api/relayMeshSignaling";
import { getIdentity } from "@/shared/api/tauri";
import {
  meshEnsureClientNode,
  type MeshServeTarget,
} from "@/shared/api/tauriMesh";

function normalizePubkey(value: string | null | undefined): string | null {
  const normalized = value?.trim().toLowerCase() ?? "";
  return /^[0-9a-f]{64}$/.test(normalized) ? normalized : null;
}

export async function startRelayMeshClientForTarget(
  modelId: string,
  target: MeshServeTarget | null,
): Promise<void> {
  const status = await meshEnsureClientNode(modelId, target);
  if (!target) {
    throw new Error(
      "Selected relay mesh target is missing its reporter pubkey.",
    );
  }
  const targetPubkey = normalizePubkey(target.reporterPubkey);
  if (!targetPubkey) {
    throw new Error(
      "Selected relay mesh target is missing its reporter pubkey.",
    );
  }
  if (!status.inviteToken) {
    throw new Error("Local mesh client did not publish an endpoint address.");
  }

  const selfPubkey = normalizePubkey((await getIdentity()).pubkey);
  if (selfPubkey === targetPubkey) {
    // The selected serve target belongs to this desktop. `meshEnsureClientNode`
    // has already ensured the local mesh ingress is usable; a relay
    // connect-request to ourselves would be rejected as self-targeting and is
    // unnecessary for local routing.
    return;
  }

  await publishMeshConnectRequest({
    targetPubkey,
    selfEndpointAddr: status.inviteToken,
    peerEndpointAddr: target.endpointAddr,
    attemptId: crypto.randomUUID(),
    selfEndpointId: status.endpointId ?? null,
    peerEndpointId: target.endpointId ?? null,
  });
}
