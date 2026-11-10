/**
 * Device-local signed OVL/DRC stake operations (`SignedStakeTx` v1).
 * Keys stay in the Agora BIP-44 vault (secp256k1). Ticket-aware v2 is not
 * built here — DRC ticket sequence stays on the account-lane builders.
 */

import { addressHrpForNetwork, encodeAddress } from "./address.ts";
import type { LightClient } from "./rpc.ts";
import {
  accountFromMnemonic,
  accountNonce,
  bytesToHex,
  concat,
  encodeVec,
  hexToBytes,
  jsonAddress,
  jsonBytes,
  NATIVE_ASSET_DRC,
  NATIVE_ASSET_OVL,
  nodeBinding,
  parseRecipient,
  requireAddress,
  requireHash,
  signBound,
  u16,
  u32,
  u64,
  u8,
  type BuiltTypedEnvelope,
  type NativeAssetTicker,
} from "./typed-lanes.ts";
import { chainIdForNetwork } from "./wallet.ts";

export const STAKE_TX_DOMAIN = new TextEncoder().encode(
  "agora-trident-stake-tx-v1",
);

export const STAKE_TX_VERSION = 1;

export type StakeOpKindName = "Bond" | "Delegate" | "UnbondSelf" | "Withdraw";

export const STAKE_OP_KINDS: ReadonlyArray<{
  id: StakeOpKindName;
  label: string;
  needsAmount: boolean;
  needsValidator: boolean;
  needsBondFields: boolean;
}> = [
  { id: "Bond", label: "Bond self", needsAmount: true, needsValidator: false, needsBondFields: true },
  { id: "Delegate", label: "Delegate", needsAmount: true, needsValidator: true, needsBondFields: false },
  { id: "UnbondSelf", label: "Unbond self", needsAmount: false, needsValidator: false, needsBondFields: false },
  { id: "Withdraw", label: "Withdraw matured", needsAmount: false, needsValidator: false, needsBondFields: false },
];

const STAKE_KIND_WIRE: Record<StakeOpKindName, number> = {
  Bond: 0,
  Delegate: 1,
  UnbondSelf: 2,
  Withdraw: 3,
};

const ASSET_WIRE: Record<NativeAssetTicker, number> = {
  OVL: NATIVE_ASSET_OVL,
  DRC: NATIVE_ASSET_DRC,
};

export function stakeKindMeta(kind: StakeOpKindName) {
  const meta = STAKE_OP_KINDS.find((entry) => entry.id === kind);
  if (!meta) throw new Error(`unknown stake kind ${kind}`);
  return meta;
}

export function requireCompressedPubkey(bytes: Uint8Array): Uint8Array {
  if (bytes.length !== 33) {
    throw new Error("consensus pubkey must be 33-byte compressed secp256k1");
  }
  if (bytes[0] !== 0x02 && bytes[0] !== 0x03) {
    throw new Error("consensus pubkey must start with 0x02 or 0x03");
  }
  return bytes;
}

export function encodeStakeTxBody(fields: {
  version: number;
  asset: NativeAssetTicker;
  kind: StakeOpKindName;
  actor: Uint8Array;
  validator: Uint8Array;
  amount: number | bigint;
  consensusPubkey: Uint8Array;
  withdrawal: Uint8Array;
  commissionBps: number;
  metadataHash: Uint8Array;
  nonce: number | bigint;
}): Uint8Array {
  return concat([
    u32(fields.version),
    u8(ASSET_WIRE[fields.asset]),
    u8(STAKE_KIND_WIRE[fields.kind]),
    requireAddress(fields.actor),
    requireAddress(fields.validator),
    u64(fields.amount),
    encodeVec(fields.consensusPubkey),
    requireAddress(fields.withdrawal),
    u16(fields.commissionBps),
    requireHash(fields.metadataHash),
    u64(fields.nonce),
  ]);
}

export async function buildSignedStakeTx(options: {
  mnemonic: string;
  accountIndex?: number;
  network?: string;
  genesisHash: string;
  chainId?: string;
  asset: NativeAssetTicker;
  kind: StakeOpKindName;
  amount?: number;
  validatorAddress?: string;
  withdrawalAddress?: string;
  consensusPubkeyHex?: string;
  commissionBps?: number;
  metadataHash?: string;
  nonce: number;
}): Promise<BuiltTypedEnvelope> {
  if (options.asset !== "OVL" && options.asset !== "DRC") {
    throw new Error("stake asset must be OVL or DRC");
  }
  const meta = stakeKindMeta(options.kind);
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(
    options.mnemonic,
    network,
    options.accountIndex ?? 0,
  );
  const actor = parseRecipient(account.addressHex, network).bytes;
  const chainId = options.chainId ?? chainIdForNetwork(network);
  const amount = meta.needsAmount ? Math.floor(options.amount ?? 0) : 0;
  if (meta.needsAmount && amount <= 0) {
    throw new Error("stake amount must be > 0");
  }
  let validator: Uint8Array = actor;
  if (meta.needsValidator) {
    const raw = options.validatorAddress?.trim();
    if (!raw) throw new Error("delegate requires a validator address");
    validator = parseRecipient(raw, network).bytes;
  }
  let withdrawal: Uint8Array = new Uint8Array(20);
  let consensusPubkey: Uint8Array = new Uint8Array(0);
  let commissionBps = 0;
  if (meta.needsBondFields) {
    const withdrawalRaw = options.withdrawalAddress?.trim();
    withdrawal = withdrawalRaw
      ? parseRecipient(withdrawalRaw, network).bytes
      : actor;
    commissionBps = Math.floor(options.commissionBps ?? 0);
    if (commissionBps < 0 || commissionBps > 10_000) {
      throw new Error("commission_bps must be 0..=10000");
    }
    consensusPubkey = options.consensusPubkeyHex?.trim()
      ? requireCompressedPubkey(hexToBytes(options.consensusPubkeyHex.trim()))
      : requireCompressedPubkey(account.publicKey);
  }
  const metadataHash = options.metadataHash?.trim()
    ? requireHash(hexToBytes(options.metadataHash.trim()))
    : new Uint8Array(32);
  const body = encodeStakeTxBody({
    version: STAKE_TX_VERSION,
    asset: options.asset,
    kind: options.kind,
    actor,
    validator,
    amount,
    consensusPubkey,
    withdrawal,
    commissionBps,
    metadataHash,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    STAKE_TX_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  const tx = {
    version: STAKE_TX_VERSION,
    asset: options.asset,
    kind: options.kind,
    actor: jsonAddress(actor),
    validator: jsonAddress(validator),
    amount,
    consensus_pubkey: jsonBytes(consensusPubkey),
    withdrawal: jsonAddress(withdrawal),
    commission_bps: commissionBps,
    metadata_hash: jsonBytes(metadataHash),
    nonce: options.nonce,
    public_key: jsonBytes(signed.publicKey),
    signature: jsonBytes(signed.signature),
  };
  const toHex = bytesToHex(validator);
  return {
    tx,
    from: account.addressHex,
    fromBech32: account.addressBech32,
    to: toHex,
    toBech32: encodeAddress(toHex, addressHrpForNetwork(network)),
    amount,
    fee: 0,
    signingBytes: signed.signingBytes,
  };
}

export async function sendStakeTx(
  client: LightClient,
  options: {
    mnemonic: string;
    accountIndex?: number;
    network?: string;
    asset: NativeAssetTicker;
    kind: StakeOpKindName;
    amount?: number;
    validatorAddress?: string;
    withdrawalAddress?: string;
    consensusPubkeyHex?: string;
    commissionBps?: number;
    metadataHash?: string;
    nonce?: number;
  },
): Promise<{ id: string; built: BuiltTypedEnvelope }> {
  const network = options.network ?? "mainnet";
  const bind = await nodeBinding(client, network);
  const account = accountFromMnemonic(
    options.mnemonic,
    bind.network,
    options.accountIndex ?? 0,
  );
  const nonce =
    options.nonce ?? (await accountNonce(client, account.addressHex, options.asset));
  const built = await buildSignedStakeTx({
    ...options,
    nonce,
    genesisHash: bind.genesisHash,
    chainId: bind.chainId,
    network: bind.network,
  });
  const submitted = await client.submitStakeTx(built.tx);
  return { id: submitted.stake_tx_id, built };
}
