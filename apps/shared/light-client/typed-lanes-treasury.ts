/**
 * Device-local signed protocol treasury disbursements.
 * Keys stay in the Agora BIP-44 vault. Does not mint.
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
import { TREASURY_IDS, type TreasuryIdName } from "./typed-lanes-community.ts";
import { chainIdForNetwork } from "./wallet.ts";

export const TREASURY_DISBURSEMENT_DOMAIN = new TextEncoder().encode(
  "agora-treasury-disbursement-v1",
);

export function encodeTreasuryDisbursementBody(fields: {
  version: number;
  treasury: TreasuryIdName;
  beneficiary: Uint8Array;
  amount: number | bigint;
  reasonHash: Uint8Array;
  authorizationRoot: Uint8Array;
  nonce: number | bigint;
}): Uint8Array {
  return concat([
    u32(fields.version),
    u8(TREASURY_IDS[fields.treasury]),
    requireAddress(fields.beneficiary),
    u64(fields.amount),
    requireHash(fields.reasonHash),
    requireHash(fields.authorizationRoot),
    u64(fields.nonce),
  ]);
}

export async function buildSignedTreasuryDisbursement(options: {
  mnemonic: string;
  accountIndex?: number;
  network?: string;
  genesisHash: string;
  chainId?: string;
  treasury: TreasuryIdName;
  beneficiary: string;
  amount: number;
  reasonHash: string;
  authorizationRoot: string;
  nonce: number;
}): Promise<BuiltTypedEnvelope> {
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(
    options.mnemonic,
    network,
    options.accountIndex ?? 0,
  );
  const beneficiary = parseRecipient(options.beneficiary, network).bytes;
  const chainId = options.chainId ?? chainIdForNetwork(network);
  const reason = requireHash(hexToBytes(options.reasonHash));
  const authRoot = requireHash(hexToBytes(options.authorizationRoot));
  const body = encodeTreasuryDisbursementBody({
    version: 1,
    treasury: options.treasury,
    beneficiary,
    amount: options.amount,
    reasonHash: reason,
    authorizationRoot: authRoot,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    TREASURY_DISBURSEMENT_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  const tx = {
    version: 1,
    treasury: options.treasury,
    beneficiary: jsonAddress(beneficiary),
    amount: options.amount,
    reason_hash: jsonBytes(reason),
    authorization_root: jsonBytes(authRoot),
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

export async function sendTreasuryDisbursement(
  client: LightClient,
  options: {
    mnemonic: string;
    accountIndex?: number;
    network?: string;
    treasury: TreasuryIdName;
    beneficiary: string;
    amount: number;
    reasonHash: string;
    authorizationRoot?: string;
    nonce?: number;
  },
): Promise<{ id: string; built: BuiltTypedEnvelope }> {
  const network = options.network ?? "mainnet";
  const bind = await nodeBinding(client, network);
  const treasuries = await client.getProtocolTreasuries();
  const authorizationRoot =
    options.authorizationRoot ??
    treasuries.policy?.authorization_root ??
    "";
  if (!authorizationRoot) {
    throw new Error("treasury authorization root is required");
  }
  const nonce =
    options.nonce ?? (await client.getTreasuryNonce(options.treasury)).nonce;
  const built = await buildSignedTreasuryDisbursement({
    ...options,
    nonce,
    authorizationRoot,
    genesisHash: bind.genesisHash,
    chainId: bind.chainId,
    network: bind.network,
  });
  const submitted = await client.submitTreasuryDisbursement(built.tx);
  return { id: submitted.disbursement_id, built };
}
