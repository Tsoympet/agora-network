/**
 * Device-local signed vesting unlock claims.
 * Keys stay in the Agora BIP-44 vault. Does not mint.
 * Time source is the including block header timestamp, not blue-score.
 */

import { addressHrpForNetwork, encodeAddress } from "./address.ts";
import type { LightClient } from "./rpc.ts";
import {
  accountFromMnemonic,
  bytesToHex,
  concat,
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
  u32,
  u64,
  u8,
  type BuiltTypedEnvelope,
} from "./typed-lanes.ts";
import { chainIdForNetwork } from "./wallet.ts";

export const VESTING_UNLOCK_DOMAIN = new TextEncoder().encode(
  "agora-vesting-unlock-v1",
);

export const NATIVE_ASSET_TLT = 0x00;

const ASSET_WIRE: Record<"TLT" | "OVL" | "DRC", number> = {
  TLT: NATIVE_ASSET_TLT,
  OVL: NATIVE_ASSET_OVL,
  DRC: NATIVE_ASSET_DRC,
};

export function encodeVestingUnlockBody(fields: {
  version: number;
  asset: "TLT" | "OVL" | "DRC";
  beneficiary: Uint8Array;
  scheduleId: Uint8Array;
  amount: number | bigint;
  nonce: number | bigint;
}): Uint8Array {
  return concat([
    u32(fields.version),
    u8(ASSET_WIRE[fields.asset]),
    requireAddress(fields.beneficiary),
    requireHash(fields.scheduleId),
    u64(fields.amount),
    u64(fields.nonce),
  ]);
}

export async function buildSignedVestingUnlock(options: {
  mnemonic: string;
  accountIndex?: number;
  network?: string;
  genesisHash: string;
  chainId?: string;
  asset: "TLT" | "OVL" | "DRC";
  scheduleId: string;
  amount: number;
  nonce: number;
}): Promise<BuiltTypedEnvelope> {
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(
    options.mnemonic,
    network,
    options.accountIndex ?? 0,
  );
  const beneficiary = parseRecipient(account.addressHex, network).bytes;
  const chainId = options.chainId ?? chainIdForNetwork(network);
  const scheduleId = requireHash(hexToBytes(options.scheduleId));
  const body = encodeVestingUnlockBody({
    version: 1,
    asset: options.asset,
    beneficiary,
    scheduleId,
    amount: options.amount,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    VESTING_UNLOCK_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  const tx = {
    version: 1,
    asset: options.asset,
    beneficiary: jsonAddress(beneficiary),
    schedule_id: jsonBytes(scheduleId),
    amount: options.amount,
    nonce: options.nonce,
    public_key: jsonBytes(signed.publicKey),
    signature: jsonBytes(signed.signature),
  };
  return {
    tx,
    from: account.addressHex,
    fromBech32: account.addressBech32,
    to: bytesToHex(beneficiary),
    toBech32: encodeAddress(bytesToHex(beneficiary), addressHrpForNetwork(network)),
    amount: options.amount,
    fee: 0,
    signingBytes: signed.signingBytes,
  };
}

export async function sendVestingUnlock(
  client: LightClient,
  options: {
    mnemonic: string;
    accountIndex?: number;
    network?: string;
    asset: "TLT" | "OVL" | "DRC";
    scheduleId: string;
    amount: number;
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
    options.nonce ?? (await client.getVestingNonce(account.addressHex)).nonce;
  const built = await buildSignedVestingUnlock({
    ...options,
    nonce,
    genesisHash: bind.genesisHash,
    chainId: bind.chainId,
    network: bind.network,
  });
  const submitted = await client.submitVestingUnlock(built.tx);
  return { id: submitted.unlock_id, built };
}
