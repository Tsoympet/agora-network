/**
 * Opt-in raw OVL-EVM helper. This is **not** the Agora BIP-44 vault.
 *
 * Agora mnemonic keys produce SHA-256/Bech32m addresses and sign Borsh
 * envelopes. Ethereum keys produce keccak addresses and sign RLP. Never
 * import a vault mnemonic here; the caller supplies an explicit secp256k1
 * scalar that they already treat as an EVM key.
 */

import { keccak_256 } from "@noble/hashes/sha3";
import * as secp from "@noble/secp256k1";
import type { LightClient } from "./rpc.ts";

export const RAW_EVM_DEV_CHAIN_ID = 74_000;
export const RAW_EVM_TESTNET_CHAIN_ID = 74_001;

export const RAW_EVM_KEY_SPLIT = {
  agoraVault:
    "BIP-39 → BIP-44 m/44'/8888'/account'/change/index → SHA-256(compressed secp256k1)[:20] Bech32m",
  rawEvm:
    "explicit 32-byte secp256k1 scalar supplied by the caller → keccak256(uncompressed pubkey without 0x04)[12:] hex",
} as const;

function hexToBytes(hex: string): Uint8Array {
  const s = hex.startsWith("0x") || hex.startsWith("0X") ? hex.slice(2) : hex;
  if (s.length % 2 !== 0) throw new Error("invalid hex");
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

function concat(parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

function parseSecret(secret: string | Uint8Array): Uint8Array {
  const bytes = typeof secret === "string" ? hexToBytes(secret) : secret;
  if (bytes.length !== 32) throw new Error("raw EVM key must be 32 bytes");
  return bytes;
}

/** RLP string (already-encoded integers/bytes, not a list). */
export function rlpBytes(data: Uint8Array): Uint8Array {
  if (data.length === 1 && data[0] < 0x80) return data;
  if (data.length <= 55) return concat([Uint8Array.of(0x80 + data.length), data]);
  const len = rlpLength(data.length);
  return concat([Uint8Array.of(0xb7 + len.length), len, data]);
}

export function rlpUint(value: number | bigint): Uint8Array {
  const n = BigInt(value);
  if (n < 0n) throw new Error("RLP integer must be non-negative");
  if (n === 0n) return Uint8Array.of(0x80);
  const hex = n.toString(16);
  const padded = hex.length % 2 === 0 ? hex : `0${hex}`;
  return rlpBytes(hexToBytes(padded));
}

export function rlpList(items: Uint8Array[]): Uint8Array {
  const payload = concat(items);
  if (payload.length <= 55) return concat([Uint8Array.of(0xc0 + payload.length), payload]);
  const len = rlpLength(payload.length);
  return concat([Uint8Array.of(0xf7 + len.length), len, payload]);
}

function rlpLength(n: number): Uint8Array {
  const hex = n.toString(16);
  const padded = hex.length % 2 === 0 ? hex : `0${hex}`;
  return hexToBytes(padded);
}

function rlpAddress(to: Uint8Array | null): Uint8Array {
  if (!to) return Uint8Array.of(0x80);
  if (to.length !== 20) throw new Error("EVM address must be 20 bytes");
  return rlpBytes(to);
}

function rlpU256(bytes: Uint8Array): Uint8Array {
  if (bytes.length !== 32) throw new Error("value must be 32 bytes");
  let i = 0;
  while (i < bytes.length && bytes[i] === 0) i += 1;
  return rlpBytes(bytes.slice(i));
}

export function ethereumAddressFromSecret(secret: string | Uint8Array): string {
  const key = parseSecret(secret);
  const uncompressed = secp.getPublicKey(key, false);
  const hash = keccak_256(uncompressed.slice(1));
  return bytesToHex(hash.slice(12));
}

async function signPrehash(
  secret: Uint8Array,
  sighash: Uint8Array,
): Promise<{ y: number; r: Uint8Array; s: Uint8Array }> {
  const signature = await secp.signAsync(sighash, secret, { lowS: true });
  const compact =
    signature instanceof Uint8Array
      ? signature.slice(0, 64)
      : signature.toCompactRawBytes();
  const recovery =
    signature instanceof Uint8Array
      ? undefined
      : signature.recovery;
  if (recovery !== 0 && recovery !== 1) {
    throw new Error("raw EVM signature missing y-parity");
  }
  return { y: recovery, r: compact.slice(0, 32), s: compact.slice(32, 64) };
}

export type RawLegacyFields = {
  chainId: number;
  nonce: number;
  gasPrice: number | bigint;
  gasLimit: number;
  to: string | Uint8Array | null;
  value?: Uint8Array;
  data?: Uint8Array;
};

export type RawEip1559Fields = {
  chainId: number;
  nonce: number;
  maxPriorityFeePerGas: number | bigint;
  maxFeePerGas: number | bigint;
  gasLimit: number;
  to: string | Uint8Array | null;
  value?: Uint8Array;
  data?: Uint8Array;
};

function dest(to: string | Uint8Array | null): Uint8Array | null {
  if (to === null) return null;
  return typeof to === "string" ? hexToBytes(to) : to;
}

export async function signLegacyRawTransaction(
  secret: string | Uint8Array,
  fields: RawLegacyFields,
): Promise<string> {
  const key = parseSecret(secret);
  const to = dest(fields.to);
  const value = fields.value ?? new Uint8Array(32);
  const data = fields.data ?? new Uint8Array();
  const unsigned = rlpList([
    rlpUint(fields.nonce),
    rlpUint(fields.gasPrice),
    rlpUint(fields.gasLimit),
    rlpAddress(to),
    rlpU256(value),
    rlpBytes(data),
    rlpUint(fields.chainId),
    rlpUint(0),
    rlpUint(0),
  ]);
  const { y, r, s } = await signPrehash(key, keccak_256(unsigned));
  const v = BigInt(fields.chainId) * 2n + 35n + BigInt(y);
  return bytesToHex(
    rlpList([
      rlpUint(fields.nonce),
      rlpUint(fields.gasPrice),
      rlpUint(fields.gasLimit),
      rlpAddress(to),
      rlpU256(value),
      rlpBytes(data),
      rlpUint(v),
      rlpBytes(r),
      rlpBytes(s),
    ]),
  );
}

export async function signEip1559RawTransaction(
  secret: string | Uint8Array,
  fields: RawEip1559Fields,
): Promise<string> {
  const key = parseSecret(secret);
  const to = dest(fields.to);
  const value = fields.value ?? new Uint8Array(32);
  const data = fields.data ?? new Uint8Array();
  const access = rlpList([]);
  const body = [
    rlpUint(fields.chainId),
    rlpUint(fields.nonce),
    rlpUint(fields.maxPriorityFeePerGas),
    rlpUint(fields.maxFeePerGas),
    rlpUint(fields.gasLimit),
    rlpAddress(to),
    rlpU256(value),
    rlpBytes(data),
    access,
  ];
  const preimage = concat([Uint8Array.of(0x02), rlpList(body)]);
  const { y, r, s } = await signPrehash(key, keccak_256(preimage));
  const signed = rlpList([...body, rlpUint(y), rlpBytes(r), rlpBytes(s)]);
  return bytesToHex(concat([Uint8Array.of(0x02), signed]));
}

export async function sendRawEvmTransaction(
  client: LightClient,
  rawHex: string,
): Promise<string> {
  const result = await client.sendRawEvmTransaction(rawHex);
  if (typeof result === "string") return result;
  throw new Error("eth_sendRawTransaction did not return a hash");
}

export async function signAndSendLegacyRawTransaction(
  client: LightClient,
  secret: string | Uint8Array,
  fields: RawLegacyFields,
): Promise<{ raw: string; hash: string; from: string }> {
  const raw = await signLegacyRawTransaction(secret, fields);
  const hash = await sendRawEvmTransaction(client, raw);
  return { raw, hash, from: ethereumAddressFromSecret(secret) };
}

export async function signAndSendEip1559RawTransaction(
  client: LightClient,
  secret: string | Uint8Array,
  fields: RawEip1559Fields,
): Promise<{ raw: string; hash: string; from: string }> {
  const raw = await signEip1559RawTransaction(secret, fields);
  const hash = await sendRawEvmTransaction(client, raw);
  return { raw, hash, from: ethereumAddressFromSecret(secret) };
}
