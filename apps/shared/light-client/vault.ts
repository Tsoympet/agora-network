/**
 * Password-sealed BIP-39 vault (AES-256-GCM + PBKDF2-SHA-256).
 *
 * Ciphertext is safe to persist in localStorage / AsyncStorage.
 * The mnemonic exists in plaintext only while unlocked in memory.
 */

import {
  generateMnemonic as bip39Generate,
  validateMnemonic as bip39Validate,
} from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english";

function validateMnemonic(mnemonic: string): boolean {
  return bip39Validate(mnemonic, wordlist);
}

const VAULT_VERSION = 1 as const;
const PBKDF2_ITERATIONS = 210_000;
const SALT_BYTES = 16;
const IV_BYTES = 12;

export const DEFAULT_VAULT_STORAGE_KEY = "agora.wallet.vault.v1";

export type SealedVault = {
  v: typeof VAULT_VERSION;
  /** base64 PBKDF2 salt */
  salt: string;
  /** base64 AES-GCM IV */
  iv: string;
  /** base64 ciphertext (UTF-8 mnemonic) */
  ciphertext: string;
  /** PBKDF2 iteration count used at seal time */
  iterations: number;
};

export type VaultStorage = {
  load(): string | null | Promise<string | null>;
  save(raw: string): void | Promise<void>;
  clear(): void | Promise<void>;
};

function requireSubtle(): SubtleCrypto {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) {
    throw new Error("Web Crypto SubtleCrypto is required for the wallet vault");
  }
  return subtle;
}

function bytesToBase64(bytes: Uint8Array): string {
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin);
}

function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/** Copy into a fresh ArrayBuffer so Web Crypto typings accept it under strict TS. */
function asBufferSource(bytes: Uint8Array): Uint8Array<ArrayBuffer> {
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return copy;
}

async function deriveAesKey(
  password: string,
  salt: Uint8Array,
  iterations: number,
): Promise<CryptoKey> {
  const subtle = requireSubtle();
  const material = await subtle.importKey(
    "raw",
    new TextEncoder().encode(password),
    "PBKDF2",
    false,
    ["deriveKey"],
  );
  return subtle.deriveKey(
    {
      name: "PBKDF2",
      salt: asBufferSource(salt),
      iterations,
      hash: "SHA-256",
    },
    material,
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt", "decrypt"],
  );
}

function normalizeMnemonic(mnemonic: string): string {
  return mnemonic.trim().toLowerCase().replace(/\s+/g, " ");
}

function normalizePassword(password: string): string {
  if (password.length < 8) {
    throw new Error("password must be at least 8 characters");
  }
  return password;
}

/** Encrypt a BIP-39 mnemonic under `password`. */
export async function sealVault(
  mnemonic: string,
  password: string,
): Promise<SealedVault> {
  const phrase = normalizeMnemonic(mnemonic);
  if (!validateMnemonic(phrase)) {
    throw new Error("invalid BIP-39 mnemonic");
  }
  const pw = normalizePassword(password);
  const salt = globalThis.crypto.getRandomValues(new Uint8Array(SALT_BYTES));
  const iv = globalThis.crypto.getRandomValues(new Uint8Array(IV_BYTES));
  const key = await deriveAesKey(pw, salt, PBKDF2_ITERATIONS);
  const ciphertext = await requireSubtle().encrypt(
    { name: "AES-GCM", iv: asBufferSource(iv) },
    key,
    new TextEncoder().encode(phrase),
  );
  return {
    v: VAULT_VERSION,
    salt: bytesToBase64(salt),
    iv: bytesToBase64(iv),
    ciphertext: bytesToBase64(new Uint8Array(ciphertext)),
    iterations: PBKDF2_ITERATIONS,
  };
}

/** Decrypt a sealed vault; throws on wrong password / corrupt blob. */
export async function openVault(
  sealed: SealedVault,
  password: string,
): Promise<string> {
  if (sealed.v !== VAULT_VERSION) {
    throw new Error(`unsupported vault version ${sealed.v}`);
  }
  const iterations = sealed.iterations || PBKDF2_ITERATIONS;
  const key = await deriveAesKey(
    password,
    base64ToBytes(sealed.salt),
    iterations,
  );
  let plain: ArrayBuffer;
  try {
    plain = await requireSubtle().decrypt(
      { name: "AES-GCM", iv: asBufferSource(base64ToBytes(sealed.iv)) },
      key,
      asBufferSource(base64ToBytes(sealed.ciphertext)),
    );
  } catch {
    throw new Error("wrong password or corrupt vault");
  }
  const mnemonic = new TextDecoder().decode(plain);
  if (!validateMnemonic(mnemonic)) {
    throw new Error("vault plaintext is not a valid mnemonic");
  }
  return mnemonic;
}

export function serializeVault(sealed: SealedVault): string {
  return JSON.stringify(sealed);
}

export function parseVault(raw: string): SealedVault {
  const parsed = JSON.parse(raw) as SealedVault;
  if (
    !parsed ||
    parsed.v !== VAULT_VERSION ||
    typeof parsed.salt !== "string" ||
    typeof parsed.iv !== "string" ||
    typeof parsed.ciphertext !== "string"
  ) {
    throw new Error("invalid vault blob");
  }
  return parsed;
}

export function localStorageVault(
  key: string = DEFAULT_VAULT_STORAGE_KEY,
): VaultStorage {
  return {
    load() {
      if (typeof localStorage === "undefined") return null;
      return localStorage.getItem(key);
    },
    save(raw: string) {
      if (typeof localStorage === "undefined") {
        throw new Error("localStorage unavailable");
      }
      localStorage.setItem(key, raw);
    },
    clear() {
      if (typeof localStorage === "undefined") return;
      localStorage.removeItem(key);
    },
  };
}

/**
 * Adapter for Expo SecureStore / AsyncStorage-style APIs:
 * `getItem` / `setItem` / `deleteItem` (or `removeItem`).
 */
export function keyValueVault(
  store: {
    getItem(key: string): Promise<string | null>;
    setItem(key: string, value: string): Promise<void>;
    deleteItem?(key: string): Promise<void>;
    removeItem?(key: string): Promise<void>;
  },
  key: string = DEFAULT_VAULT_STORAGE_KEY,
): VaultStorage {
  return {
    load: () => store.getItem(key),
    save: (raw) => store.setItem(key, raw),
    clear: async () => {
      if (store.deleteItem) await store.deleteItem(key);
      else if (store.removeItem) await store.removeItem(key);
    },
  };
}

export async function loadSealedVault(
  storage: VaultStorage,
): Promise<SealedVault | null> {
  const raw = await storage.load();
  if (!raw) return null;
  return parseVault(raw);
}

export async function persistSealedVault(
  storage: VaultStorage,
  sealed: SealedVault,
): Promise<void> {
  await storage.save(serializeVault(sealed));
}

export async function clearPersistedVault(storage: VaultStorage): Promise<void> {
  await storage.clear();
}

/** Round-trip self-check for Node / CI (requires Web Crypto). */
export async function vaultSelfTest(): Promise<void> {
  const mnemonic = bip39Generate(wordlist, 128);
  const password = "test-pass-ok";
  const sealed = await sealVault(mnemonic, password);
  const opened = await openVault(sealed, password);
  if (opened !== mnemonic) {
    throw new Error("vault round-trip mismatch");
  }
  let rejected = false;
  try {
    await openVault(sealed, "wrong-password!!");
  } catch {
    rejected = true;
  }
  if (!rejected) {
    throw new Error("vault accepted wrong password");
  }
}
