/**
 * PC vault policy on top of the existing AES-GCM localStorage blob.
 * Ciphertext format stays v1 so already sealed vaults still open.
 * A longer KDF is a future vault version, not a silent rewrite of v1.
 */

export const DEFAULT_SESSION_TIMEOUT_MS = 5 * 60 * 1000;

export const SESSION_TIMEOUT_CHOICES = [60_000, 300_000, 900_000] as const;

export const VAULT_KDF = {
  version: 1 as const,
  algorithm: "AES-256-GCM",
  kdf: "PBKDF2-SHA-256",
  iterations: 210_000,
  storage: "localStorage",
  nextKdf: "PLANNED" as const,
};

export interface HardwareSigner {
  readonly status: "PLANNED";
  readonly transport: "usb";
  listAccounts(): Promise<never>;
  signPreview(): Promise<never>;
}

export function plannedHardwareSigner(): HardwareSigner {
  const refuse = () =>
    Promise.reject(new Error("PLANNED: no USB hardware signer is linked on this build"));
  return {
    status: "PLANNED",
    transport: "usb",
    listAccounts: refuse,
    signPreview: refuse,
  };
}
