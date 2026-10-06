/**
 * Device-cache passport statements.
 * The consensus domain is `agora-passport-attestation-v1` (borsh, in
 * agora-crypto). This module does not claim those bytes. Chain inclusion
 * stays PLANNED until a proof is checked on the device.
 */

import { sha256 } from "@noble/hashes/sha256";
import * as secp from "@noble/secp256k1";

import { bytesToHex, hexToBytes } from "../core/hex";

export const CLIENT_PASSPORT_DOMAIN = "agora-community-client-passport-v1";

export const CONSENSUS_PASSPORT_VERIFICATION = "PLANNED" as const;

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

export type ClientAttestation = {
  domain: typeof CLIENT_PASSPORT_DOMAIN;
  issuer: string;
  subject: string;
  category: PassportCategoryName;
  evidenceHash: string;
  nonce: string;
  signatureHex: string;
  publicKeyHex: string;
  chainInclusion: "PLANNED";
};

export type NonceLedger = Map<string, Set<string>>;

export function createNonceLedger(): NonceLedger {
  return new Map();
}

function message(fields: Pick<ClientAttestation, "issuer" | "subject" | "category" | "evidenceHash" | "nonce">): string {
  return [
    CLIENT_PASSPORT_DOMAIN,
    fields.issuer.trim(),
    fields.subject.trim(),
    fields.category,
    fields.evidenceHash.trim(),
    fields.nonce.trim(),
  ].join("\n");
}

function compactSignature(signature: Uint8Array | { toCompactRawBytes: () => Uint8Array }): Uint8Array {
  return signature instanceof Uint8Array ? signature.slice(0, 64) : signature.toCompactRawBytes();
}

export async function signClientAttestation(
  secretKey: Uint8Array,
  fields: Pick<ClientAttestation, "issuer" | "subject" | "category" | "evidenceHash" | "nonce">,
): Promise<ClientAttestation> {
  if (!PASSPORT_CATEGORIES.includes(fields.category)) throw new Error("unknown passport category");
  if (!/^[0-9a-f]{64}$/i.test(fields.evidenceHash)) throw new Error("evidence hash must be 32 bytes");
  if (!fields.nonce.trim()) throw new Error("nonce required");
  const digest = sha256(new TextEncoder().encode(message(fields)));
  const signature = await secp.signAsync(digest, secretKey, { lowS: true });
  return {
    domain: CLIENT_PASSPORT_DOMAIN,
    issuer: fields.issuer.trim(),
    subject: fields.subject.trim(),
    category: fields.category,
    evidenceHash: fields.evidenceHash.trim(),
    nonce: fields.nonce.trim(),
    signatureHex: bytesToHex(compactSignature(signature)),
    publicKeyHex: bytesToHex(secp.getPublicKey(secretKey, true)),
    chainInclusion: "PLANNED",
  };
}

export async function acceptClientAttestation(
  ledger: NonceLedger,
  attestation: ClientAttestation,
): Promise<ClientAttestation> {
  if (attestation.domain !== CLIENT_PASSPORT_DOMAIN) {
    throw new Error("unexpected passport domain");
  }
  if (attestation.chainInclusion !== "PLANNED") {
    throw new Error("client cache cannot mark chain inclusion");
  }
  const digest = sha256(new TextEncoder().encode(message(attestation)));
  const ok = secp.verify(
    hexToBytes(attestation.signatureHex),
    digest,
    hexToBytes(attestation.publicKeyHex),
  );
  if (!ok) throw new Error("passport signature rejected");
  const seen = ledger.get(attestation.issuer) ?? new Set<string>();
  if (seen.has(attestation.nonce)) {
    throw new Error("passport nonce replay");
  }
  seen.add(attestation.nonce);
  ledger.set(attestation.issuer, seen);
  return { ...attestation, chainInclusion: "PLANNED" };
}

/** Evidence references only. A numeric reputation score is not invented here. */
export function reputationEvidence(accepted: readonly ClientAttestation[]): {
  evidence: Array<{ category: PassportCategoryName; nonce: string }>;
  score: "PLANNED";
  likesIgnored: true;
} {
  return {
    evidence: accepted.map((item) => ({ category: item.category, nonce: item.nonce })),
    score: "PLANNED",
    likesIgnored: true,
  };
}
