/**
 * Pairwise TLT transaction Merkle proofs.
 * Leaf pairing matches `Block::compute_tx_root` / `tlt_tx_merkle_root`.
 * UTXO-only headers store this root in `header.tx_root`. Multi-lane bodies wrap it.
 */
import { sha256 } from "@noble/hashes/sha256";

export type TltTxMerkleProof = {
  index: number;
  txId: Uint8Array;
  siblings: Uint8Array[];
};

function cloneBytes(bytes: Uint8Array): Uint8Array {
  return new Uint8Array(bytes);
}

function hashPair(left: Uint8Array, right: Uint8Array): Uint8Array {
  const buf = new Uint8Array(64);
  buf.set(left, 0);
  buf.set(right, 32);
  return cloneBytes(sha256(buf));
}

export function tltTxMerkleRoot(txIds: Uint8Array[]): Uint8Array {
  if (txIds.length === 0) return new Uint8Array(32);
  let level: Uint8Array[] = txIds.map(cloneBytes);
  while (level.length > 1) {
    if (level.length % 2 === 1) level.push(cloneBytes(level[level.length - 1]));
    const next: Uint8Array[] = [];
    for (let i = 0; i < level.length; i += 2) {
      next.push(hashPair(level[i], level[i + 1]));
    }
    level = next;
  }
  return level[0];
}

export function proveTltTxMerkle(
  txIds: Uint8Array[],
  index: number,
): TltTxMerkleProof | null {
  if (index < 0 || index >= txIds.length) return null;
  let idx = index;
  let level: Uint8Array[] = txIds.map(cloneBytes);
  const siblings: Uint8Array[] = [];
  while (level.length > 1) {
    if (level.length % 2 === 1) level.push(cloneBytes(level[level.length - 1]));
    siblings.push(cloneBytes(level[idx ^ 1]));
    const next: Uint8Array[] = [];
    for (let i = 0; i < level.length; i += 2) {
      next.push(hashPair(level[i], level[i + 1]));
    }
    level = next;
    idx = Math.floor(idx / 2);
  }
  return { index, txId: cloneBytes(txIds[index]), siblings };
}

export function verifyTltTxMerkle(root: Uint8Array, proof: TltTxMerkleProof): boolean {
  let hash = cloneBytes(proof.txId);
  let idx = proof.index;
  for (const sibling of proof.siblings) {
    hash = idx % 2 === 0 ? hashPair(hash, sibling) : hashPair(sibling, hash);
    idx = Math.floor(idx / 2);
  }
  if (hash.length !== root.length) return false;
  for (let i = 0; i < hash.length; i += 1) {
    if (hash[i] !== root[i]) return false;
  }
  return true;
}

export type TridentLightState =
  | "Proposed"
  | "PoWAccepted"
  | "AwaitingOvlQuorum"
  | "AwaitingDrcQuorum"
  | "Finalized";

function hasTwoThirds(signed: bigint, active: bigint): boolean {
  if (active <= 0n) return false;
  return signed * 3n >= active * 2n;
}

/** PoW plus independent OVL and DRC two-thirds quorums. Empty stake never passes. */
export function tridentLightState(input: {
  powWorkMet: boolean;
  ovlSigned: bigint;
  ovlActive: bigint;
  drcSigned: bigint;
  drcActive: bigint;
}): TridentLightState {
  if (!input.powWorkMet) return "Proposed";
  const ovl = hasTwoThirds(input.ovlSigned, input.ovlActive);
  const drc = hasTwoThirds(input.drcSigned, input.drcActive);
  if (ovl && drc) return "Finalized";
  if (!ovl && drc) return "AwaitingOvlQuorum";
  if (ovl && !drc) return "AwaitingDrcQuorum";
  return "PoWAccepted";
}

/**
 * Check TLT inclusion and report Trident finality from supplied stake totals.
 * Pass `headerTxRoot` only for UTXO-only blocks, where it equals the tx Merkle root.
 */
export function verifyTridentLight(input: {
  txMerkleRoot: Uint8Array;
  inclusion: TltTxMerkleProof;
  headerTxRoot?: Uint8Array | null;
  powWorkMet: boolean;
  ovlSigned: bigint;
  ovlActive: bigint;
  drcSigned: bigint;
  drcActive: bigint;
}): TridentLightState {
  if (!verifyTltTxMerkle(input.txMerkleRoot, input.inclusion)) {
    throw new Error("TLT merkle inclusion failed");
  }
  if (input.headerTxRoot) {
    const header = input.headerTxRoot;
    const root = input.txMerkleRoot;
    if (header.length !== root.length || header.some((byte, i) => byte !== root[i])) {
      throw new Error("UTXO-only header tx_root does not match the TLT merkle root");
    }
  }
  return tridentLightState(input);
}
