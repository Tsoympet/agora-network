/**
 * Device-local signed Hub-coordinator passport attestations.
 * Keys stay in the Agora BIP-44 vault. Domain: agora-passport-attestation-v1.
 */

import { addressHrpForNetwork, encodeAddress } from "./address.ts";
import type { LightClient } from "./rpc.ts";
import {
  accountFromMnemonic,
  bytesToHex,
  concat,
  encodeBoundEnvelope,
  encodeOptionU64,
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
  type BuiltTypedEnvelope,
} from "./typed-lanes.ts";
import { chainIdForNetwork } from "./wallet.ts";

export const PASSPORT_ATTESTATION_DOMAIN = new TextEncoder().encode(
  "agora-passport-attestation-v1",
);

export const PASSPORT_CATEGORIES = [
  "Code",
  "Documentation",
  "Translation",
  "SecurityReport",
  "Infrastructure",
  "EventHosting",
  "Teaching",
  "MerchantOnboarding",
  "GrantReview",
  "GovernanceParticipation",
  "Moderation",
  "CommunitySupport",
] as const;

export type PassportCategoryName = (typeof PASSPORT_CATEGORIES)[number];

export function passportCategoryDiscriminant(
  category: PassportCategoryName,
): number {
  const index = PASSPORT_CATEGORIES.indexOf(category);
  if (index < 0) throw new Error(`unknown passport category ${category}`);
  return index;
}

export function encodePassportAttestationBody(fields: {
  version: number;
  issuer: Uint8Array;
  subject: Uint8Array;
  category: PassportCategoryName;
  evidenceHash: Uint8Array;
  issuerPolicyHash: Uint8Array;
  issuedEpoch: number | bigint;
  expiresEpoch: number | bigint | null;
  nonce: number | bigint;
}): Uint8Array {
  return concat([
    u32(fields.version),
    requireAddress(fields.issuer),
    requireAddress(fields.subject),
    Uint8Array.of(passportCategoryDiscriminant(fields.category)),
    requireHash(fields.evidenceHash),
    requireHash(fields.issuerPolicyHash),
    u64(fields.issuedEpoch),
    encodeOptionU64(fields.expiresEpoch),
    u64(fields.nonce),
  ]);
}

export async function buildSignedPassportAttestation(options: {
  mnemonic: string;
  accountIndex?: number;
  network?: string;
  genesisHash: string;
  chainId?: string;
  subject: string;
  category: PassportCategoryName;
  evidenceHash: string;
  issuerPolicyHash: string;
  issuedEpoch: number;
  expiresEpoch?: number | null;
  nonce: number;
}): Promise<BuiltTypedEnvelope> {
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(
    options.mnemonic,
    network,
    options.accountIndex ?? 0,
  );
  const issuer = hexToBytes(account.addressHex);
  const subject = parseRecipient(options.subject, network).bytes;
  const evidence = requireHash(hexToBytes(options.evidenceHash));
  const policy = requireHash(hexToBytes(options.issuerPolicyHash));
  const chainId = options.chainId ?? chainIdForNetwork(network);
  const body = encodePassportAttestationBody({
    version: 1,
    issuer,
    subject,
    category: options.category,
    evidenceHash: evidence,
    issuerPolicyHash: policy,
    issuedEpoch: options.issuedEpoch,
    expiresEpoch: options.expiresEpoch ?? null,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    PASSPORT_ATTESTATION_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  const tx = {
    version: 1,
    issuer: jsonAddress(issuer),
    subject: jsonAddress(subject),
    category: options.category,
    evidence_hash: jsonBytes(evidence),
    issuer_policy_hash: jsonBytes(policy),
    issued_epoch: options.issuedEpoch,
    expires_epoch: options.expiresEpoch ?? null,
    nonce: options.nonce,
    public_key: jsonBytes(signed.publicKey),
    signature: jsonBytes(signed.signature),
  };
  return {
    tx,
    from: account.addressHex,
    fromBech32: account.addressBech32,
    to: bytesToHex(subject),
    toBech32: encodeAddress(bytesToHex(subject), addressHrpForNetwork(network)),
    amount: 0,
    fee: 0,
    signingBytes: signed.signingBytes,
  };
}

export async function sendPassportAttestation(
  client: LightClient,
  options: {
    mnemonic: string;
    accountIndex?: number;
    network?: string;
    subject: string;
    category: PassportCategoryName;
    evidenceHash: string;
    issuerPolicyHash: string;
    issuedEpoch: number;
    expiresEpoch?: number | null;
    nonce?: number;
  },
): Promise<{ id: string; built: BuiltTypedEnvelope }> {
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(
    options.mnemonic,
    network,
    options.accountIndex ?? 0,
  );
  const bind = await nodeBinding(client, network);
  const nonce =
    options.nonce ??
    (await client.getPassportIssuerNonce(account.addressBech32)).nonce;
  const built = await buildSignedPassportAttestation({
    ...options,
    nonce,
    genesisHash: bind.genesisHash,
    chainId: bind.chainId,
    network: bind.network,
  });
  const submitted = await client.submitPassportAttestation(built.tx);
  return { id: submitted.attestation_id, built };
}
