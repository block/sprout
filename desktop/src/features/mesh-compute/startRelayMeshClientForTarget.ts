import { publishMeshConnectRequest } from "@/shared/api/relayMeshSignaling";
import {
  meshEnsureClientNode,
  type MeshServeTarget,
} from "@/shared/api/tauriMesh";

export async function startRelayMeshClientForTarget(
  modelId: string,
  target: MeshServeTarget | null,
): Promise<void> {
  const status = await meshEnsureClientNode(modelId, target);
  if (!target?.reporterPubkey) {
    throw new Error(
      "Selected relay mesh target is missing its reporter pubkey.",
    );
  }
  if (!status.inviteToken) {
    throw new Error("Local mesh client did not publish an endpoint address.");
  }
  await publishMeshConnectRequest({
    targetPubkey: target.reporterPubkey,
    selfEndpointAddr: status.inviteToken,
    peerEndpointAddr: target.endpointAddr,
    attemptId: crypto.randomUUID(),
    selfEndpointId: status.endpointId ?? null,
    peerEndpointId: target.endpointId ?? null,
  });
}
