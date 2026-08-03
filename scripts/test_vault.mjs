#!/usr/bin/env node
/**
 * Smoke: AES-GCM wallet vault round-trip (Node Web Crypto).
 * Mirrors apps/shared/light-client/vault.ts without TypeScript strip imports.
 */
import {
  generateMnemonic,
  validateMnemonic,
} from "../apps/shared/node_modules/@scure/bip39/index.js";
import { wordlist } from "../apps/shared/node_modules/@scure/bip39/wordlists/english.js";

const PBKDF2_ITERATIONS = 210_000;

function bytesToBase64(bytes) {
  return Buffer.from(bytes).toString("base64");
}
function base64ToBytes(b64) {
  return new Uint8Array(Buffer.from(b64, "base64"));
}

async function deriveAesKey(password, salt, iterations) {
  const material = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(password),
    "PBKDF2",
    false,
    ["deriveKey"],
  );
  return crypto.subtle.deriveKey(
    { name: "PBKDF2", salt, iterations, hash: "SHA-256" },
    material,
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt", "decrypt"],
  );
}

async function seal(mnemonic, password) {
  const phrase = mnemonic.trim().toLowerCase().replace(/\s+/g, " ");
  if (!validateMnemonic(phrase, wordlist)) throw new Error("bad mnemonic");
  if (password.length < 8) throw new Error("short password");
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const key = await deriveAesKey(password, salt, PBKDF2_ITERATIONS);
  const ciphertext = await crypto.subtle.encrypt(
    { name: "AES-GCM", iv },
    key,
    new TextEncoder().encode(phrase),
  );
  return {
    salt: bytesToBase64(salt),
    iv: bytesToBase64(iv),
    ciphertext: bytesToBase64(new Uint8Array(ciphertext)),
  };
}

async function open(sealed, password) {
  const key = await deriveAesKey(
    password,
    base64ToBytes(sealed.salt),
    PBKDF2_ITERATIONS,
  );
  const plain = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv: base64ToBytes(sealed.iv) },
    key,
    base64ToBytes(sealed.ciphertext),
  );
  return new TextDecoder().decode(plain);
}

const mnemonic = generateMnemonic(wordlist, 128);
const sealed = await seal(mnemonic, "test-pass-ok");
const opened = await open(sealed, "test-pass-ok");
if (opened !== mnemonic) throw new Error("round-trip mismatch");
let rejected = false;
try {
  await open(sealed, "wrong-password!!");
} catch {
  rejected = true;
}
if (!rejected) throw new Error("accepted wrong password");
console.log("vault self-test OK");
