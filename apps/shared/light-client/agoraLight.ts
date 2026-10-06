/**
 * Entire-network light checks shared by the desktop and mobile wallets.
 *
 * Header hashes and TLT body bindings are recomputed locally. RandomX proof of
 * work and validator signatures are not checked here; those stay on full nodes.
 * Missing proofs fail closed.
 */
import { sha256 } from "@noble/hashes/sha256";

import { normalizeNetworkId, type AgoraNetworkId } from "./network";
import { tridentLightState, verifyTltTxMerkle, type TridentLightState } from "./tltMerkle";

export type LightHeaderFields = {
  version: number;
  parents: string[];
  timestamp_ms: number;
  bits: number;
  nonce: number;
  tx_root: string;
};

export type LightHeaderRecord = {
  hash: string;
  selected_parent: string | null;
  blue_score: number;
  is_genesis?: boolean;
  header: LightHeaderFields;
};

export type BodyBindingStep =
  | { kind: "v2"; account_ids: string[]; stake_ids: string[] }
  | { kind: "v3"; execution_ids: string[] }
  | { kind: "v4"; payment_ids: string[] }
  | { kind: "versioned"; domain: string; version: number; id_lists: string[][] };

export type LightHeaderChain = {
  network: string;
  genesis: string;
  tip: string;
  order: "tip_to_genesis";
  reaches_genesis: boolean;
  pow_checked_by: string;
  headers: LightHeaderRecord[];
  finality: LightFinalityReport;
};

export type LightFinalityReport = {
  block_hash: string;
  node_state?: string;
  pow_work_met: boolean;
  ovl_signed_stake?: string | number | null;
  ovl_active_stake?: string | number | null;
  drc_signed_stake?: string | number | null;
  drc_active_stake?: string | number | null;
  stake_fields_present: boolean;
};

export type TltInclusionProof = {
  hash: string;
  header: LightHeaderFields;
  tx_merkle_root: string;
  utxo_only: boolean;
  binding: BodyBindingStep[];
  inclusion: { index: number; tx_id: string; siblings: string[] };
  ovl_execution_ids?: string[];
  drc_payment_ids?: string[];
  programmable_lane?: string;
  drc_contract_lane?: boolean;
};

export type TltInclusionResponse = {
  tx_id: string;
  status: string;
  on_virtual_spine: boolean;
  proof: TltInclusionProof | null;
};

export type NativeAssetBalance = {
  asset: "TLT" | "OVL" | "DRC";
  module: "utxo" | "account";
  balance: string;
  nonce?: string;
  record?: "present" | "absent";
  header_proven: false;
};

export type NativeBalances = {
  address: string;
  address_hex: string;
  assets: {
    TLT: NativeAssetBalance;
    OVL: NativeAssetBalance;
    DRC: NativeAssetBalance;
  };
};

export type SpineRelation = "same" | "extended" | "reorg" | "mismatch";

function hexToBytes(hex: string): Uint8Array {
  const clean = hex.startsWith("0x") ? hex.slice(2) : hex;
  if (clean.length !== 64 || clean.length % 2 !== 0) {
    throw new Error("hash must be 32 bytes");
  }
  const out = new Uint8Array(32);
  for (let i = 0; i < 32; i += 1) {
    const byte = Number.parseInt(clean.slice(i * 2, i * 2 + 2), 16);
    if (Number.isNaN(byte)) throw new Error("malformed hash");
    out[i] = byte;
  }
  return out;
}

function bytesToHex(bytes: Uint8Array): string {
  return [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

function concat(parts: Uint8Array[]): Uint8Array {
  const size = parts.reduce((sum, part) => sum + part.length, 0);
  const out = new Uint8Array(size);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

function u16(value: number): Uint8Array {
  const out = new Uint8Array(2);
  new DataView(out.buffer).setUint16(0, value, true);
  return out;
}

function u32(value: number): Uint8Array {
  const out = new Uint8Array(4);
  new DataView(out.buffer).setUint32(0, value, true);
  return out;
}

function u64(value: number | bigint): Uint8Array {
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, BigInt(value), true);
  return out;
}

function vecHashes(ids: string[]): Uint8Array {
  return concat([u32(ids.length), ...ids.map(hexToBytes)]);
}

/** Borsh encoding of `BlockHeader`, then SHA-256. Matches `BlockHeader::hash`. */
export function hashLightHeader(header: LightHeaderFields): string {
  const parents = header.parents.map(hexToBytes);
  const body = concat([
    u16(header.version),
    u32(parents.length),
    ...parents,
    u64(header.timestamp_ms),
    u32(header.bits),
    u64(header.nonce),
    hexToBytes(header.tx_root),
  ]);
  return bytesToHex(sha256(body));
}

export function verifySelectedParentSpine(
  tipToGenesis: LightHeaderRecord[],
  expectedGenesis: string,
): string {
  if (tipToGenesis.length === 0) throw new Error("empty header spine");
  const seen = new Set<string>();
  const genesis = expectedGenesis.toLowerCase();
  for (let index = 0; index < tipToGenesis.length; index += 1) {
    const record = tipToGenesis[index];
    const hash = hashLightHeader(record.header);
    if (hash !== record.hash.toLowerCase()) {
      throw new Error("header hash mismatch");
    }
    if (seen.has(hash)) throw new Error("duplicate header");
    seen.add(hash);
    const isAnchor = index + 1 === tipToGenesis.length;
    if (isAnchor) {
      if (hash !== genesis) throw new Error("wrong network genesis");
      if (record.selected_parent) throw new Error("genesis anchor has a selected parent");
      continue;
    }
    const parent = hashLightHeader(tipToGenesis[index + 1].header);
    if ((record.selected_parent ?? "").toLowerCase() !== parent) {
      throw new Error("selected-parent ancestry mismatch");
    }
    if (!record.header.parents.some((item) => item.toLowerCase() === parent)) {
      throw new Error("selected parent is not a header parent");
    }
    if (record.blue_score <= tipToGenesis[index + 1].blue_score) {
      throw new Error("blue score did not increase along the selected parent");
    }
  }
  return tipToGenesis[0].hash.toLowerCase();
}

function sameRecord(left: LightHeaderRecord, right: LightHeaderRecord): boolean {
  return (
    hashLightHeader(left.header) === hashLightHeader(right.header) &&
    (left.selected_parent ?? "") === (right.selected_parent ?? "") &&
    left.blue_score === right.blue_score &&
    left.header.tx_root === right.header.tx_root
  );
}

export function compareHeaderSpines(
  previous: LightHeaderRecord[],
  next: LightHeaderRecord[],
): SpineRelation {
  if (previous.length === 0) return next.length === 0 ? "same" : "extended";
  const nextByHash = new Map<string, LightHeaderRecord>();
  for (const record of next) {
    const hash = hashLightHeader(record.header);
    if (hash !== record.hash.toLowerCase()) throw new Error("header hash mismatch");
    const prior = nextByHash.get(hash);
    if (prior && !sameRecord(prior, record)) return "mismatch";
    nextByHash.set(hash, record);
  }
  for (const record of previous) {
    const hash = hashLightHeader(record.header);
    const other = nextByHash.get(hash);
    if (other && !sameRecord(record, other)) return "mismatch";
  }
  const prevTip = hashLightHeader(previous[0].header);
  const nextTip = next.length === 0 ? null : hashLightHeader(next[0].header);
  if (nextTip === prevTip) return "same";
  if (nextByHash.has(prevTip)) return "extended";
  return "reorg";
}

export function foldBodyBinding(txMerkleRootHex: string, steps: BodyBindingStep[]): string {
  let inner = hexToBytes(txMerkleRootHex);
  for (const step of steps) {
    inner = sha256(encodeStep(inner, step));
  }
  return bytesToHex(inner);
}

function encodeStep(inner: Uint8Array, step: BodyBindingStep): Uint8Array {
  if (step.kind === "v2") {
    return concat([
      new TextEncoder().encode("agora-block-body-v2"),
      inner,
      vecHashes(step.account_ids),
      vecHashes(step.stake_ids),
    ]);
  }
  if (step.kind === "v3") {
    return concat([
      new TextEncoder().encode("agora-block-body-v3"),
      inner,
      vecHashes(step.execution_ids),
    ]);
  }
  if (step.kind === "v4") {
    return concat([
      new TextEncoder().encode("agora-block-body-v4"),
      inner,
      vecHashes(step.payment_ids),
    ]);
  }
  if (step.id_lists.length < 1 || step.id_lists.length > 4) {
    throw new Error("unsupported body binding");
  }
  const domain = new TextEncoder().encode(step.domain);
  return concat([
    u32(domain.length),
    domain,
    u16(step.version),
    inner,
    ...step.id_lists.map(vecHashes),
  ]);
}

function parseStake(value: string | number | null | undefined, label: string): bigint {
  if (value === null || value === undefined || value === "") {
    throw new Error(`missing ${label}`);
  }
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value) || value < 0) throw new Error(`malformed ${label}`);
    return BigInt(value);
  }
  if (!/^\d+$/.test(value)) throw new Error(`malformed ${label}`);
  return BigInt(value);
}

/** Recompute finality from explicit stake totals. Missing totals fail closed. */
export function verifyReportedFinality(
  report: LightFinalityReport,
  headerHash: string,
): TridentLightState {
  if (report.block_hash.toLowerCase() !== headerHash.toLowerCase()) {
    throw new Error("finality header mismatch");
  }
  if (!report.stake_fields_present) {
    throw new Error("missing quorum totals");
  }
  return tridentLightState({
    powWorkMet: report.pow_work_met,
    ovlSigned: parseStake(report.ovl_signed_stake, "ovl signed stake"),
    ovlActive: parseStake(report.ovl_active_stake, "ovl active stake"),
    drcSigned: parseStake(report.drc_signed_stake, "drc signed stake"),
    drcActive: parseStake(report.drc_active_stake, "drc active stake"),
  });
}

export function assertExpectedNetwork(
  reported: string | null | undefined,
  expected: AgoraNetworkId | string,
): void {
  if (!reported) throw new Error("node did not report a network");
  if (normalizeNetworkId(reported) !== normalizeNetworkId(expected)) {
    throw new Error("wrong network");
  }
}

/**
 * Verify a TLT transaction against its block header.
 * Pending, unknown, and proof-less responses throw.
 */
export function verifyIncomingTlt(
  response: TltInclusionResponse,
  spine?: LightHeaderRecord[],
): { txId: string; headerHash: string; txMerkleRoot: string } {
  if (!response.proof) throw new Error("missing TLT inclusion proof");
  if (response.status !== "confirmed" || !response.on_virtual_spine) {
    throw new Error("transaction is not confirmed on the selected-parent spine");
  }
  const proof = response.proof;
  if (proof.drc_contract_lane) throw new Error("DRC contract lane is not part of Agora");
  if (proof.programmable_lane && proof.programmable_lane !== "OVL") {
    throw new Error("programmable lane must be OVL");
  }
  const headerHash = hashLightHeader(proof.header);
  if (headerHash !== proof.hash.toLowerCase()) throw new Error("header hash mismatch");
  if (spine && !spine.some((record) => record.hash.toLowerCase() === headerHash)) {
    throw new Error("inclusion block is not on the verified header spine");
  }
  const txId = hexToBytes(proof.inclusion.tx_id);
  const siblings = proof.inclusion.siblings.map(hexToBytes);
  const merkle = hexToBytes(proof.tx_merkle_root);
  if (
    !verifyTltTxMerkle(merkle, {
      index: proof.inclusion.index,
      txId,
      siblings,
    })
  ) {
    throw new Error("TLT merkle inclusion failed");
  }
  if (proof.inclusion.tx_id.toLowerCase() !== response.tx_id.toLowerCase()) {
    throw new Error("inclusion tx id mismatch");
  }
  const folded = foldBodyBinding(proof.tx_merkle_root, proof.binding);
  if (folded !== proof.header.tx_root.toLowerCase()) {
    throw new Error("header tx_root does not match the TLT body binding");
  }
  return {
    txId: response.tx_id.toLowerCase(),
    headerHash,
    txMerkleRoot: proof.tx_merkle_root.toLowerCase(),
  };
}

/** Prove an OVL execution id or DRC lane id is inside a header body binding. */
export function verifyLaneId(proof: TltInclusionProof, id: string): void {
  if (!id) throw new Error("missing lane id");
  const headerHash = hashLightHeader(proof.header);
  if (headerHash !== proof.hash.toLowerCase()) throw new Error("header hash mismatch");
  const folded = foldBodyBinding(proof.tx_merkle_root, proof.binding);
  if (folded !== proof.header.tx_root.toLowerCase()) {
    throw new Error("header tx_root does not match the body binding");
  }
  const needle = id.toLowerCase();
  const lists = proof.binding.flatMap((step) => {
    if (step.kind === "v2") return [...step.account_ids, ...step.stake_ids];
    if (step.kind === "v3") return step.execution_ids;
    if (step.kind === "v4") return step.payment_ids;
    return step.id_lists.flat();
  });
  if (!lists.some((item) => item.toLowerCase() === needle)) {
    throw new Error("lane id is not in the header binding");
  }
}

/**
 * DRC ledger objects are canonical full-node reads. There is no header proof
 * for object state, so a caller that asks for one fails closed.
 */
export function verifyDrcObjectHeaderProof(witness: unknown): never {
  const headerProof =
    witness !== null &&
    typeof witness === "object" &&
    "header_proof" in witness
      ? (witness as { header_proof?: unknown }).header_proof
      : undefined;
  if (headerProof == null) {
    throw new Error("missing DRC object header proof");
  }
  throw new Error("DRC object header proofs are not committed in block headers");
}

export function readNativeBalances(raw: NativeBalances): NativeBalances {
  const assets = raw.assets;
  for (const ticker of ["TLT", "OVL", "DRC"] as const) {
    const row = assets[ticker];
    if (!row || row.asset !== ticker) throw new Error(`missing ${ticker} balance`);
    if (row.header_proven !== false) {
      throw new Error(`${ticker} balance was marked header-proven without a proof`);
    }
    if (!/^\d+$/.test(row.balance)) throw new Error(`malformed ${ticker} balance`);
  }
  if (assets.TLT.module !== "utxo") throw new Error("TLT balance must come from the UTXO set");
  if (assets.OVL.module !== "account" || assets.DRC.module !== "account") {
    throw new Error("OVL and DRC balances must come from account state");
  }
  return raw;
}
