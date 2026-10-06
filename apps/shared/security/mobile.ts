/**
 * Phone key handling. The sealed vault already goes through Expo SecureStore.
 * Biometrics and hardware-backed keys are not linked on this build, so they
 * stay PLANNED instead of reporting a false secure-enclave status.
 */

import { sha256 } from "@noble/hashes/sha256";

import { bytesToHex, hexToBytes } from "../core/hex";
import { DEFAULT_VAULT_STORAGE_KEY } from "../light-client/vault";

export const SECURE_STORE_VAULT_KEY = DEFAULT_VAULT_STORAGE_KEY;

export const MOBILE_KEY_POLICY = {
  vault: "Expo SecureStore receives only the AES-GCM vault blob, via keyValueVault.",
  keysLeaveDevice: false as const,
  hardwareBackedKeys: "PLANNED" as const,
  hardwareReason:
    "This Expo build does not bind keys to a secure element. The mnemonic stays inside the sealed vault until the vault password opens it.",
};

export type BiometricGate = {
  status: "PLANNED";
  available: false;
  reason: string;
};

export const HARDWARE_BIOMETRIC: BiometricGate = {
  status: "PLANNED",
  available: false,
  reason: "No hardware biometric module is linked. A PIN hash can lock the app UI and never stores the seed.",
};

export type PinRecord = {
  saltHex: string;
  hashHex: string;
  holdsSeed: false;
};

export function createPinRecord(pin: string): PinRecord {
  if (!/^[0-9]{4,8}$/.test(pin)) throw new Error("PIN must be 4 to 8 digits");
  const salt = globalThis.crypto.getRandomValues(new Uint8Array(16));
  const hash = sha256(concat(salt, new TextEncoder().encode(pin)));
  return { saltHex: bytesToHex(salt), hashHex: bytesToHex(hash), holdsSeed: false };
}

export function pinMatches(record: PinRecord, pin: string): boolean {
  const salt = hexToBytes(record.saltHex);
  const hash = sha256(concat(salt, new TextEncoder().encode(pin)));
  const got = bytesToHex(hash);
  if (got.length !== record.hashHex.length) return false;
  let diff = 0;
  for (let i = 0; i < got.length; i += 1) {
    diff |= got.charCodeAt(i) ^ record.hashHex.charCodeAt(i);
  }
  return diff === 0;
}

function concat(left: Uint8Array, right: Uint8Array): Uint8Array {
  const out = new Uint8Array(left.length + right.length);
  out.set(left, 0);
  out.set(right, left.length);
  return out;
}

/**
 * iOS and Android screenshot blocking needs a native flag this app does not
 * ship. The available control is to keep the mnemonic hidden until Show.
 * Web can only mark the document; that does not stop a screenshot.
 */
export const SCREENSHOT_PRIVACY = {
  android: "PLANNED",
  ios: "PLANNED",
  web: "best-effort",
} as const;

export type ScreenshotPlatform = keyof typeof SCREENSHOT_PRIVACY;

export function mnemonicScreenPrivacy(platform: ScreenshotPlatform): {
  hideUntilExplicitReveal: true;
  nativeBlock: (typeof SCREENSHOT_PRIVACY)[ScreenshotPlatform];
} {
  return {
    hideUntilExplicitReveal: true,
    nativeBlock: SCREENSHOT_PRIVACY[platform],
  };
}
