/**
 * Browser/mobile wallet helpers matching `agora-crypto`:
 * BIP-39 seed → BIP-44 m/44'/8888'/0'/0/index → secp256k1 sign of borsh(TransactionBody).
 */

import { sha256 } from "@noble/hashes/sha256";
import * as secp from "@noble/secp256k1";
import { HDKey } from "@scure/bip32";
import {
  generateMnemonic as bip39Generate,
  mnemonicToSeedSync,
  validateMnemonic as bip39Validate,
} from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english";

export { wordlist };
export const validateMnemonic = (mnemonic: string): boolean =>
  bip39Validate(mnemonic.trim().toLowerCase().replace(/\s+/g, " "), wordlist);

/** Fresh BIP-39 mnemonic (`strength` 128 → 12 words, 256 → 24). */
export function generateMnemonic(strength: 128 | 256 = 128): string {
  return bip39Generate(wordlist, strength);
}

import {
  ADDRESS_HRP,
  addressHrpForNetwork,
  encodeAddress,
  parseAddress,
} from "./address";
import type { LightClient, LightUtxo } from "./rpc";

/** Provisional SLIP-0044 coin type — replace before mainnet freeze. */
export const AGORA_COIN_TYPE = 8888;
export const AGORA_COIN_TYPE_PROVISIONAL = AGORA_COIN_TYPE;

export type WalletAccount = {
  index: number;
  /** Canonical 40-char lowercase hex (consensus payload). */
  addressHex: string;
  /** Bech32m display form (`agora1…`). */
  addressBech32: string;
  publicKey: Uint8Array;
  secretKey: Uint8Array;
};

export type BuiltTransfer = {
  /** Native-serde JSON body for `agora_submitTransaction`. */
  tx: Record<string, unknown>;
  /** Sender hex (consensus). */
  from: string;
  /** Sender Bech32m display. */
  fromBech32: string;
  /** Destination hex. */
  to: string;
  /** Destination Bech32m. */
  toBech32: string;
  amount: number;
  change: number;
  fee: number;
};

function hexToBytes(hex: string): Uint8Array {
  const s = hex.startsWith("0x") ? hex.slice(2) : hex;
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

function writeU32(view: DataView, offset: number, value: number): number {
  view.setUint32(offset, value >>> 0, true);
  return offset + 4;
}

function writeU64(view: DataView, offset: number, value: number | bigint): number {
  view.setBigUint64(offset, BigInt(value), true);
  return offset + 8;
}

/** Borsh encoding of `TransactionBody` (must match Rust `agora_types`). */
export function encodeTransactionBody(body: {
  version: number;
  inputs: { tx_id: Uint8Array; index: number }[];
  outputs: { value: number; address: Uint8Array }[];
  nonce: number | bigint;
}): Uint8Array {
  // version + inputs_len + outputs_len + nonce
  let size = 4 + 4 + 4 + 8;
  for (const _input of body.inputs) size += 32 + 4;
  for (const _output of body.outputs) size += 8 + 20;
  const buf = new ArrayBuffer(size);
  const view = new DataView(buf);
  const bytes = new Uint8Array(buf);
  let o = 0;
  o = writeU32(view, o, body.version);
  o = writeU32(view, o, body.inputs.length);
  for (const input of body.inputs) {
    if (input.tx_id.length !== 32) throw new Error("tx_id must be 32 bytes");
    bytes.set(input.tx_id, o);
    o += 32;
    o = writeU32(view, o, input.index);
  }
  o = writeU32(view, o, body.outputs.length);
  for (const output of body.outputs) {
    if (output.address.length !== 20) throw new Error("address must be 20 bytes");
    o = writeU64(view, o, output.value);
    bytes.set(output.address, o);
    o += 20;
  }
  writeU64(view, o, body.nonce);
  return bytes;
}

export function addressFromPubkey(pubkey: Uint8Array): string {
  return bytesToHex(sha256(pubkey).slice(0, 20));
}

export function addressFromMnemonic(
  mnemonic: string,
  index = 0,
  passphrase = "",
): string {
  return deriveAccount(mnemonic, index, passphrase).addressHex;
}

/** Bech32m form of the derived external address (HRP from `network`). */
export function addressBech32FromMnemonic(
  mnemonic: string,
  index = 0,
  passphrase = "",
  network = "mainnet",
): string {
  return deriveAccount(mnemonic, index, passphrase, network).addressBech32;
}

/**
 * BIP-44 account derivation.
 * @param change `0` = external (receive), `1` = internal (change)
 */
export function deriveAccount(
  mnemonic: string,
  index = 0,
  passphrase = "",
  network = "mainnet",
  change: 0 | 1 = 0,
): WalletAccount {
  const phrase = mnemonic.trim().toLowerCase().replace(/\s+/g, " ");
  if (!validateMnemonic(phrase)) {
    throw new Error("invalid BIP-39 mnemonic");
  }
  const seed = mnemonicToSeedSync(phrase, passphrase);
  const path = `m/44'/${AGORA_COIN_TYPE}'/0'/${change}/${index}`;
  const hd = HDKey.fromMasterSeed(seed).derive(path);
  if (!hd.privateKey || !hd.publicKey) {
    throw new Error("derivation failed");
  }
  // @scure/bip32 publicKey is compressed 33 bytes.
  const publicKey = hd.publicKey;
  const addressHex = addressFromPubkey(publicKey);
  const hrp = addressHrpForNetwork(network) || ADDRESS_HRP;
  return {
    index,
    addressHex,
    addressBech32: encodeAddress(addressHex, hrp),
    publicKey,
    secretKey: hd.privateKey,
  };
}

export async function signTransactionBody(
  secretKey: Uint8Array,
  bodyBytes: Uint8Array,
): Promise<{ publicKey: Uint8Array; signature: Uint8Array }> {
  const digest = sha256(bodyBytes);
  // noble v2: signAsync returns compact 64-byte sig by default with recovered bit options.
  const signature = await secp.signAsync(digest, secretKey, { lowS: true });
  const compact =
    signature instanceof Uint8Array
      ? signature.slice(0, 64)
      : (signature as { toCompactRawBytes: () => Uint8Array }).toCompactRawBytes();
  const publicKey = secp.getPublicKey(secretKey, true);
  return { publicKey, signature: compact };
}

/** Greedy coin selection + BIP-44 change; fee (`in − out`) is paid to the miner. */
export async function buildSignedTransfer(options: {
  mnemonic: string;
  accountIndex?: number;
  /** UTXOs must all be owned by the signing chain (`spendChange`). */
  utxos: LightUtxo[];
  toAddressHex: string;
  amount: number;
  fee?: number;
  nonce?: number;
  network?: string;
  /** When true, inputs are from the internal chain and signed with change keys. */
  spendChange?: boolean;
}): Promise<BuiltTransfer> {
  const accountIndex = options.accountIndex ?? 0;
  const network = options.network ?? "mainnet";
  const spendChange = options.spendChange === true;
  const account = deriveAccount(
    options.mnemonic,
    accountIndex,
    "",
    network,
    spendChange ? 1 : 0,
  );
  // Change outputs always go to the internal chain (BIP-44 change=1).
  const changeAccount = deriveAccount(
    options.mnemonic,
    accountIndex,
    "",
    network,
    1,
  );
  const fee = options.fee ?? 1;
  const need = options.amount + fee;
  if (need <= 0) throw new Error("amount must be > 0");
  const toHex = parseAddress(options.toAddressHex);
  const to = hexToBytes(toHex);
  if (to.length !== 20) throw new Error("to address must be 20 bytes");

  const sorted = [...options.utxos].sort((a, b) => b.value - a.value);
  const selected: LightUtxo[] = [];
  let totalIn = 0;
  for (const u of sorted) {
    selected.push(u);
    totalIn += u.value;
    if (totalIn >= need) break;
  }
  if (totalIn < need) {
    throw new Error(`insufficient funds: have ${totalIn}, need ${need}`);
  }
  const change = totalIn - need;
  const outputs: { value: number; address: Uint8Array }[] = [
    { value: options.amount, address: to },
  ];
  if (change > 0) {
    outputs.push({
      value: change,
      address: hexToBytes(changeAccount.addressHex),
    });
  }

  const inputs = selected.map((u) => ({
    tx_id: hexToBytes(u.tx_id),
    index: u.index,
  }));
  const version = 1;
  const nonce = options.nonce ?? Date.now();
  const bodyBytes = encodeTransactionBody({
    version,
    inputs,
    outputs,
    nonce,
  });
  const { publicKey, signature } = await signTransactionBody(
    account.secretKey,
    bodyBytes,
  );

  const tx = {
    version,
    inputs: selected.map((u) => ({
      previous_outpoint: {
        tx_id: Array.from(hexToBytes(u.tx_id)),
        index: u.index,
      },
    })),
    outputs: outputs.map((o) => ({
      value: o.value,
      address: Array.from(o.address),
    })),
    nonce,
    public_key: Array.from(publicKey),
    signature: Array.from(signature),
  };

  return {
    tx,
    from: account.addressHex,
    fromBech32: account.addressBech32,
    to: toHex,
    toBech32: encodeAddress(toHex, addressHrpForNetwork(network)),
    amount: options.amount,
    change,
    fee,
  };
}

export async function sendTransfer(
  client: LightClient,
  options: {
    mnemonic: string;
    accountIndex?: number;
    toAddressHex: string;
    amount: number;
    fee?: number;
    network?: string;
  },
): Promise<{ tx_id: string; built: BuiltTransfer }> {
  const accountIndex = options.accountIndex ?? 0;
  const network = options.network ?? "mainnet";
  let fee = options.fee;
  if (fee === undefined) {
    try {
      const est = await client.estimateFee();
      fee = est.suggested_fee;
    } catch {
      fee = 1;
    }
  }
  const receive = deriveAccount(options.mnemonic, accountIndex, "", network, 0);
  const changeAcc = deriveAccount(
    options.mnemonic,
    accountIndex,
    "",
    network,
    1,
  );
  const [ext, ch] = await Promise.all([
    client.getUtxos(receive.addressHex),
    client.getUtxos(changeAcc.addressHex),
  ]);
  const need = options.amount + fee;
  const sum = (u: { value: number }[]) => u.reduce((a, b) => a + b.value, 0);
  let utxos = ext.utxos;
  let spendChange = false;
  if (sum(ext.utxos) < need) {
    if (sum(ch.utxos) >= need) {
      utxos = ch.utxos;
      spendChange = true;
    } else if (sum(ext.utxos) + sum(ch.utxos) >= need) {
      throw new Error(
        "funds split across receive/change chains — send smaller amount or consolidate first",
      );
    }
  }
  const built = await buildSignedTransfer({
    ...options,
    fee,
    network,
    utxos,
    spendChange,
  });
  const result = await client.submitTransaction(built.tx);
  return { tx_id: result.tx_id, built };
}
