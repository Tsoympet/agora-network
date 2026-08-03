/**
 * Address string helpers matching `agora-types::Address`:
 * Bech32m HRPs `agora` / `agoratest` / `agoradev` over the 20-byte pubkey hash,
 * plus legacy 40-char hex.
 */

import { bech32m } from "@scure/base";

/** Mainnet / default encode HRP. */
export const ADDRESS_HRP = "agora";
export const ADDRESS_HRP_MAINNET = "agora";
export const ADDRESS_HRP_TESTNET = "agoratest";
export const ADDRESS_HRP_DEV = "agoradev";

const KNOWN_HRPS = new Set([
  ADDRESS_HRP_MAINNET,
  ADDRESS_HRP_TESTNET,
  ADDRESS_HRP_DEV,
]);

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

export function addressHrpForNetwork(network: string): string {
  switch (network.trim().toLowerCase()) {
    case "mainnet":
    case "main":
      return ADDRESS_HRP_MAINNET;
    case "testnet":
    case "test":
      return ADDRESS_HRP_TESTNET;
    case "devnet":
    case "dev":
    case "local":
      return ADDRESS_HRP_DEV;
    default:
      return ADDRESS_HRP_DEV;
  }
}

/** Encode a 20-byte payload (hex or bytes) as Bech32m with the given HRP. */
export function encodeAddress(
  hexOrBytes: string | Uint8Array,
  hrp: string = ADDRESS_HRP,
): string {
  const bytes =
    typeof hexOrBytes === "string" ? hexToBytes(hexOrBytes) : hexOrBytes;
  if (bytes.length !== 20) {
    throw new Error("address must be 20 bytes");
  }
  return bech32m.encode(hrp, bech32m.toWords(bytes));
}

/**
 * Parse Bech32m (`agora1…` / `agoratest1…` / `agoradev1…`) or 40-char hex
 * into canonical lowercase hex. Throws on invalid input.
 *
 * When `network` is set, Bech32 HRP must match that network (`agora` /
 * `agoratest` / `agoradev`). Raw hex stays network-neutral.
 */
export function parseAddress(input: string, network?: string): string {
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
    throw new Error(
      "invalid address (expected agora1… / agoratest1… / agoradev1… or 40-char hex)",
    );
  }
  if (!KNOWN_HRPS.has(prefix)) {
    throw new Error(
      `unexpected address HRP '${prefix}' (want agora|agoratest|agoradev)`,
    );
  }
  if (network !== undefined) {
    const expected = addressHrpForNetwork(network);
    if (prefix !== expected) {
      throw new Error(
        `address HRP '${prefix}' does not match ${network} (want ${expected}1…)`,
      );
    }
  }
  const bytes = bech32m.fromWords(words);
  if (bytes.length !== 20) {
    throw new Error("invalid address payload length");
  }
  return bytesToHex(bytes);
}

/** True when `input` is a valid Bech32m or hex Agora address. */
export function isAddress(input: string, network?: string): boolean {
  try {
    parseAddress(input, network);
    return true;
  } catch {
    return false;
  }
}

/** Abbreviate Bech32 / hex for UI (keeps HRP visible for Bech32). */
export function shortAddress(addr: string, side = 6): string {
  const s = addr.trim();
  const hrpEnd = s.indexOf("1");
  if (hrpEnd > 0 && s.length > hrpEnd + 1 + side * 2 + 1) {
    return `${s.slice(0, hrpEnd + 1 + side)}…${s.slice(-side)}`;
  }
  if (s.length <= side * 2) return s;
  return `${s.slice(0, side)}…${s.slice(-side)}`;
}
