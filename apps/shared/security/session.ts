/**
 * Spend sessions hold a mnemonic only in a side table, and only while unlocked.
 * Export is a separate explicit act. Locking and timeout both drop the secret
 * so a later export cannot succeed.
 */

import { bytesToHex } from "../core/hex";

const secrets = new WeakMap<DeviceSpendSession, string>();

export type DeviceSpendSession = {
  readonly id: string;
  locked: boolean;
  expiresAt: number;
  timeoutMs: number;
};

function randomId(): string {
  return bytesToHex(globalThis.crypto.getRandomValues(new Uint8Array(16)));
}

export function openDeviceSpendSession(
  mnemonic: string,
  now: number,
  timeoutMs: number,
): DeviceSpendSession {
  const phrase = mnemonic.trim().toLowerCase().replace(/\s+/g, " ");
  if (!phrase) throw new Error("session requires a mnemonic");
  if (timeoutMs < 1_000) throw new Error("session timeout must be at least 1 second");
  const session: DeviceSpendSession = {
    id: randomId(),
    locked: false,
    expiresAt: now + timeoutMs,
    timeoutMs,
  };
  secrets.set(session, phrase);
  return session;
}

export function lockDeviceSpendSession(session: DeviceSpendSession): void {
  session.locked = true;
  secrets.delete(session);
}

function live(session: DeviceSpendSession, now: number): boolean {
  if (session.locked || now >= session.expiresAt || !secrets.has(session)) {
    lockDeviceSpendSession(session);
    return false;
  }
  return true;
}

export function touchDeviceSpendSession(session: DeviceSpendSession, now: number): void {
  if (!live(session, now)) throw new Error("locked session cannot be refreshed");
  session.expiresAt = now + session.timeoutMs;
}

/**
 * The only seed export. Locked, expired, and non-explicit callers get nothing.
 * The returned phrase is the caller's responsibility to keep off the screen.
 */
export function exportDeviceSeed(
  session: DeviceSpendSession,
  now: number,
  explicitReveal: boolean,
): string {
  if (!live(session, now)) {
    throw new Error("locked session cannot export seed");
  }
  if (!explicitReveal) {
    throw new Error("seed export requires an explicit reveal");
  }
  const phrase = secrets.get(session);
  if (!phrase) throw new Error("locked session cannot export seed");
  return phrase;
}
