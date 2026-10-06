/** Little-endian Borsh pieces shared with `agora-types` signing preimages. */

export function concat(parts: Uint8Array[]): Uint8Array {
  const size = parts.reduce((sum, part) => sum + part.length, 0);
  const out = new Uint8Array(size);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

export function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

export function hexToBytes(hex: string): Uint8Array {
  const clean = hex.trim().toLowerCase().replace(/^0x/, "");
  if (clean.length % 2 !== 0 || !/^[0-9a-f]*$/.test(clean)) {
    throw new Error("invalid hex");
  }
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < out.length; i += 1) {
    out[i] = Number.parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

export function u8(value: number): Uint8Array {
  return Uint8Array.of(value & 0xff);
}

export function u16(value: number): Uint8Array {
  const out = new Uint8Array(2);
  new DataView(out.buffer).setUint16(0, value, true);
  return out;
}

export function u32(value: number): Uint8Array {
  const out = new Uint8Array(4);
  new DataView(out.buffer).setUint32(0, value >>> 0, true);
  return out;
}

export function u64(value: bigint): Uint8Array {
  if (value < 0n || value > 0xffff_ffff_ffff_ffffn) {
    throw new Error("u64 out of range");
  }
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, value, true);
  return out;
}

export function borshStr(value: string): Uint8Array {
  const bytes = new TextEncoder().encode(value);
  return concat([u32(bytes.length), bytes]);
}

export function borshBytes(bytes: Uint8Array): Uint8Array {
  return concat([u32(bytes.length), bytes]);
}

export function repeatByte(byte: number, length: number): Uint8Array {
  return new Uint8Array(length).fill(byte);
}

export function optionBytes(inner: Uint8Array | null): Uint8Array {
  if (!inner) return u8(0);
  return concat([u8(1), inner]);
}

/** JSON number array matching serde of `[u8; N]` and `Vec<u8>`. */
export function jsonBytes(bytes: Uint8Array): number[] {
  return Array.from(bytes);
}

export function parseU64(raw: string | undefined, label: string): bigint {
  const text = (raw ?? "").trim();
  if (!/^\d+$/.test(text)) throw new Error(`${label} must be a non-negative integer`);
  return BigInt(text);
}

export function optionalU64(raw: string | undefined): bigint | null {
  const text = (raw ?? "").trim();
  if (!text) return null;
  return parseU64(text, "value");
}

export function optionalU32(raw: string | undefined): number | null {
  const text = (raw ?? "").trim();
  if (!text) return null;
  if (!/^\d+$/.test(text)) throw new Error("tag must be a non-negative integer");
  const value = Number(text);
  if (!Number.isSafeInteger(value) || value > 0xffff_ffff) {
    throw new Error("tag does not fit in u32");
  }
  return value;
}

export function asSafeNumber(value: bigint, label: string): number {
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error(`${label} exceeds the safe JSON integer range`);
  }
  return Number(value);
}
