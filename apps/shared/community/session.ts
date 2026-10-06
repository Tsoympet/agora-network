/**
 * Community sessions are short-lived bearer tokens bound to a wallet
 * signature. They are not seeds, vault passwords, or consensus keys.
 */

import { sha256 } from "@noble/hashes/sha256";
import * as secp from "@noble/secp256k1";

export const SESSION_DOMAIN = "agora-community-session-v1";
export const SESSION_TTL_MS = 15 * 60 * 1000;

const SECRET_KEYS = new Set([
  "mnemonic",
  "seed",
  "privatekey",
  "secretkey",
  "password",
  "xprv",
  "vault",
]);

export type SessionChallenge = {
  schemaVersion: 1;
  domain: typeof SESSION_DOMAIN;
  address: string;
  nonce: string;
  issuedAt: number;
  expiresAt: number;
};

export type CommunitySession = {
  schemaVersion: 1;
  address: string;
  token: string;
  issuedAt: number;
  expiresAt: number;
  scopes: Array<"community.read" | "community.write">;
};

export type WalletMode = "signing" | "watch-only";

export function createSessionChallenge(address: string, now: number, nonce: string): SessionChallenge {
  if (!address.trim() || !nonce.trim()) throw new Error("challenge requires address and nonce");
  return {
    schemaVersion: 1,
    domain: SESSION_DOMAIN,
    address: address.trim(),
    nonce,
    issuedAt: now,
    expiresAt: now + SESSION_TTL_MS,
  };
}

export function sessionSigningMessage(challenge: SessionChallenge): string {
  return [
    challenge.domain,
    challenge.address,
    challenge.nonce,
    String(challenge.issuedAt),
    String(challenge.expiresAt),
  ].join("\n");
}

export async function signSessionChallenge(
  secretKey: Uint8Array,
  challenge: SessionChallenge,
): Promise<string> {
  const digest = sha256(new TextEncoder().encode(sessionSigningMessage(challenge)));
  const signature = await secp.signAsync(digest, secretKey, { lowS: true });
  const compact =
    signature instanceof Uint8Array
      ? signature.slice(0, 64)
      : (signature as { toCompactRawBytes: () => Uint8Array }).toCompactRawBytes();
  return bytesToHex(compact);
}

export async function verifySessionSignature(
  challenge: SessionChallenge,
  publicKey: Uint8Array,
  signatureHex: string,
  now: number,
): Promise<boolean> {
  if (now > challenge.expiresAt) return false;
  const digest = sha256(new TextEncoder().encode(sessionSigningMessage(challenge)));
  const signature = hexToBytes(signatureHex);
  if (signature.length !== 64) return false;
  return secp.verify(signature, digest, publicKey);
}

export function issueSession(address: string, now: number, token: string): CommunitySession {
  if (!/^[0-9a-f]{32,}$/.test(token)) throw new Error("session token must be an opaque hex string");
  return {
    schemaVersion: 1,
    address,
    token,
    issuedAt: now,
    expiresAt: now + SESSION_TTL_MS,
    scopes: ["community.read", "community.write"],
  };
}

export function sessionIsLive(session: CommunitySession, now: number): boolean {
  return now <= session.expiresAt && session.token.length >= 32;
}

/** The only session export. Seed material is rejected, never copied out. */
export function exportCommunitySession(input: CommunitySession): CommunitySession {
  for (const key of Object.keys(input)) {
    if (SECRET_KEYS.has(key.toLowerCase())) {
      throw new Error("session cannot export seed");
    }
  }
  const exported: CommunitySession = {
    schemaVersion: 1,
    address: input.address,
    token: input.token,
    issuedAt: input.issuedAt,
    expiresAt: input.expiresAt,
    scopes: [...input.scopes],
  };
  const blob = JSON.stringify(exported);
  if (/\b(mnemonic|seed|xprv|privatekey|secretkey)\b/i.test(blob)) {
    throw new Error("session cannot export seed");
  }
  return exported;
}

export function assertCommunitySpendAllowed(mode: WalletMode): void {
  if (mode !== "signing") {
    throw new Error("watch-only wallets cannot sign community-gated spends");
  }
}

export function createRateLimiter(max: number, windowMs: number) {
  const hits = new Map<string, number[]>();
  return {
    allow(key: string, now: number): boolean {
      const prev = (hits.get(key) ?? []).filter((t) => now - t < windowMs);
      if (prev.length >= max) {
        hits.set(key, prev);
        return false;
      }
      prev.push(now);
      hits.set(key, prev);
      return true;
    },
  };
}

export function antiSybilDecision(input: {
  signatureValid: boolean;
  rateAllowed: boolean;
  reputation: number;
  reputationThreshold: number;
}): { pass: boolean; reasons: string[] } {
  const reasons: string[] = [];
  if (!input.signatureValid) reasons.push("wallet signature required");
  if (!input.rateAllowed) reasons.push("rate limit");
  if (input.reputation < input.reputationThreshold) {
    reasons.push("reputation below threshold");
  }
  return { pass: reasons.length === 0, reasons };
}

function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function hexToBytes(hex: string): Uint8Array {
  const s = hex.startsWith("0x") ? hex.slice(2) : hex;
  if (s.length % 2 !== 0) throw new Error("invalid hex");
  const out = new Uint8Array(s.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = Number.parseInt(s.slice(i * 2, i * 2 + 2), 16);
  return out;
}
