/**
 * Light-client verifier surface. This folder does not grow a full node:
 * RandomX, validator signatures, and unproven balances stay off-device checks.
 */

export {
  verifyDrcObjectHeaderProof,
  verifyIncomingTlt,
  verifyReportedFinality,
  verifySelectedParentSpine,
} from "../light-client/agoraLight";
export { createLightClient, type LightClient } from "../light-client/rpc";
export {
  proveTltTxMerkle,
  tltTxMerkleRoot,
  verifyTltTxMerkle,
  type TltTxMerkleProof,
} from "../light-client/tltMerkle";

export const PROOF_LABELS = {
  "tlt.merkle": "verified-locally",
  "header.spine": "verified-locally",
  "drc.object": "node-reported",
  balance: "node-reported",
  "ovl.contract": "PLANNED",
} as const;

export type ProofId = keyof typeof PROOF_LABELS;

export function proofLabel(id: ProofId): (typeof PROOF_LABELS)[ProofId] {
  return PROOF_LABELS[id];
}
