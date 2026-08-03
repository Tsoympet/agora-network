/**
 * Address string helpers matching `agora-types::Address`:
 * Bech32m HRP `agora` over the 20-byte pubkey hash, plus legacy 40-char hex.
 */

import { bech32m } from "@scure/base";

export const ADDRESS_HRP = "agora";

const HEX40 = /^(0x)?[0-9a-fA-F]{40}$/;

function hexToBytes(hex: string): Uint8Array {
  const s = hex.startsWith("0x") ? hex.slice(2) : hex;
  const out = new Uint8Array(s.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = Number.parseInt(s.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

/** Encode a 20-byte payload (hex or bytes) as `agora1…` Bech32m. */
export function encodeAddress(hexOrBytes: string | Uint8Array): string {
  const bytes =
    typeof hexOrBytes === "string" ? hexToBytes(hexOrBytes) : hexOrBytes;
  if (bytes.length !== 20) {
    throw new Error("address must be 20 bytes");
  }
  return bech32m.encode(ADDRESS_HRP, bech32m.toWords(bytes));
}

/**
 * Parse Bech32m (`agora1…`) or 40-char hex into canonical lowercase hex.
 * Throws on invalid input.
 */
export function parseAddress(input: string): string {
  const s = input.trim();
  if (!s) throw new Error("empty address");
  if (HEX40.test(s)) {
    return (s.startsWith("0x") ? s.slice(2) : s).toLowerCase();
  }
  let prefix: string;
  let words: number[];
  try {
    ({ prefix, words } = bech32m.decode(s as `${string}1${string}`));
  } catch {
    throw new Error("invalid address (expected agora1… or 40-char hex)");
  }
  if (prefix !== ADDRESS_HRP) {
    throw new Error(`unexpected address HRP '${prefix}' (want ${ADDRESS_HRP})`);
  }
  const bytes = bech32m.fromWords(words);
  if (bytes.length !== 20) {
    throw new Error("invalid address payload length");
  }
  return bytesToHex(bytes);
}

/** True when `input` is a valid Bech32m or hex Agora address. */
export function isAddress(input: string): boolean {
  try {
    parseAddress(input);
    return true;
  } catch {
    return false;
  }
}
