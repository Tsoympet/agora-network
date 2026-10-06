/**
 * Optional signature around an existing merchant-pay QR.
 * The inner payload stays the community QR grammar. A flipped amount fails
 * the seal. Opening a seal does not submit a payment or invent a profile.
 */

import { sha256 } from "@noble/hashes/sha256";
import * as secp from "@noble/secp256k1";

import { parseAgoraQr } from "../community/qr";
import { bytesToHex, hexToBytes } from "../core/hex";

const SEAL_PREFIX = "agora-merchant-seal:1:";
const SEAL_DOMAIN = "agora-merchant-qr-seal-v1";

export type OpenedMerchantPay = {
  destination: string;
  amount: string;
  asset: "DRC";
  signature: "valid";
  paymentSubmitted: false;
  merchantProfile: "PLANNED";
  confirmed: false;
};

function compactSignature(signature: Uint8Array | { toCompactRawBytes: () => Uint8Array }): Uint8Array {
  return signature instanceof Uint8Array ? signature.slice(0, 64) : signature.toCompactRawBytes();
}

export async function sealMerchantPay(body: string, secretKey: Uint8Array): Promise<string> {
  const parsed = parseAgoraQr(body);
  if (!parsed.ok || parsed.payload.kind !== "merchant-pay") {
    throw new Error("seal requires a merchant-pay QR");
  }
  const digest = sha256(new TextEncoder().encode(`${SEAL_DOMAIN}\n${body}`));
  const signature = await secp.signAsync(digest, secretKey, { lowS: true });
  const sig = bytesToHex(compactSignature(signature));
  const pub = bytesToHex(secp.getPublicKey(secretKey, true));
  return `${SEAL_PREFIX}${sig}:${pub}:${body}`;
}

export async function openMerchantSeal(text: string): Promise<OpenedMerchantPay> {
  if (!text.startsWith(SEAL_PREFIX)) throw new Error("unrecognized merchant seal");
  const rest = text.slice(SEAL_PREFIX.length);
  const sigEnd = rest.indexOf(":");
  const pubEnd = rest.indexOf(":", sigEnd + 1);
  if (sigEnd < 0 || pubEnd < 0) throw new Error("malformed merchant seal");
  const sig = rest.slice(0, sigEnd);
  const pub = rest.slice(sigEnd + 1, pubEnd);
  const body = rest.slice(pubEnd + 1);
  const parsed = parseAgoraQr(body);
  if (!parsed.ok || parsed.preview.asset !== "DRC" || !parsed.preview.destination || !parsed.preview.amount) {
    throw new Error("sealed QR is not a DRC merchant payment");
  }
  const digest = sha256(new TextEncoder().encode(`${SEAL_DOMAIN}\n${body}`));
  const valid = secp.verify(hexToBytes(sig), digest, hexToBytes(pub));
  if (!valid) throw new Error("QR signature does not match");
  return {
    destination: parsed.preview.destination,
    amount: parsed.preview.amount,
    asset: "DRC",
    signature: "valid",
    paymentSubmitted: false,
    merchantProfile: "PLANNED",
    confirmed: false,
  };
}
