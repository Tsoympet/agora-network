/**
 * Device-local typed-lane builders. Keys never leave the vault; envelopes
 * match Rust `signing_bytes_bound` / covenant bound sighash (Borsh).
 *
 * These are Agora-signed lanes only. Raw Ethereum envelopes are not built here.
 */

import { addressHrpForNetwork, encodeAddress, parseAddress } from "./address.ts";
import type { LightClient, LightUtxo } from "./rpc.ts";
import { selectTltCoins } from "./coinselect.ts";
import {
  chainIdForNetwork,
  deriveAccount,
  signTransactionBody,
  type WalletAccount,
} from "./wallet.ts";

export const ACCOUNT_TX_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-account-tx-v2",
);
export const OVL_EXECUTION_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-ovl-execution-v1",
);
export const DRC_PAYMENT_V4_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-payment-v4",
);
export const DRC_OFFER_CREATE_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-offer-create-v1",
);
export const DRC_OFFER_CANCEL_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-offer-cancel-v1",
);
export const DRC_OFFER_CREATE_TX_TYPE = new TextEncoder().encode("drc_offer_create");
export const DRC_OFFER_CANCEL_TX_TYPE = new TextEncoder().encode("drc_offer_cancel");
const COVENANT_BOUND_DOMAIN = new TextEncoder().encode(
  "agora-tlt-covenant-sighash-bound-v1",
);
const COVENANT_SIGHASH_DOMAIN = new TextEncoder().encode(
  "agora-tlt-covenant-sighash-v1",
);

export const ACCOUNT_TRANSFER_VERSION = 2;
export const OVL_EXECUTION_VERSION = 1;
export const DRC_PAYMENT_VERSION = 4;
export const DRC_OFFER_CREATE_TX_VERSION = 1;
export const DRC_OFFER_CANCEL_TX_VERSION = 1;
export const TLT_COVENANT_TX_VERSION = 2;
export const TLT_SEQUENCE_FINAL = 0xffff_ffff;
export const OVL_INTRINSIC_GAS = 21_000;
export const NATIVE_ASSET_OVL = 0x01;
export const NATIVE_ASSET_DRC = 0x02;
export const DRC_BOOK_NATIVE = 1;
export const DRC_BOOK_ISSUED = 2;
export const DRC_OFFER_FILL_BUY = 1;
export const DRC_OFFER_TIF_GTC = 1;

const OP_0 = 0x00;
const OP_PUSH = 0x01;
const OP_1 = 0x51;
const OP_DUP = 0x76;
const OP_EQUALVERIFY = 0x88;
const OP_CHECKSIG = 0xac;
const OP_PUBKEYHASH = 0xa9;

export type NativeAssetTicker = "OVL" | "DRC";

export type BuiltTypedEnvelope = {
  tx: Record<string, unknown>;
  from: string;
  fromBech32: string;
  to: string;
  toBech32: string;
  amount: number;
  fee: number;
  signingBytes: Uint8Array;
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

export function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function concat(parts: Uint8Array[]): Uint8Array {
  const size = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(size);
  let o = 0;
  for (const part of parts) {
    out.set(part, o);
    o += part.length;
  }
  return out;
}

function u8(value: number): Uint8Array {
  return Uint8Array.of(value & 0xff);
}

function u32(value: number): Uint8Array {
  const buf = new ArrayBuffer(4);
  new DataView(buf).setUint32(0, value >>> 0, true);
  return new Uint8Array(buf);
}

function u64(value: number | bigint): Uint8Array {
  const buf = new ArrayBuffer(8);
  new DataView(buf).setBigUint64(0, BigInt(value), true);
  return new Uint8Array(buf);
}

/** Borsh `Vec<u8>` / `&[u8]`. */
export function encodeVec(bytes: Uint8Array): Uint8Array {
  return concat([u32(bytes.length), bytes]);
}

/** Borsh `String`. */
export function encodeString(value: string): Uint8Array {
  return encodeVec(new TextEncoder().encode(value));
}

function encodeOptionU32(value: number | null | undefined): Uint8Array {
  if (value === null || value === undefined) return u8(0);
  return concat([u8(1), u32(value)]);
}

function encodeOptionU64(value: number | bigint | null | undefined): Uint8Array {
  if (value === null || value === undefined) return u8(0);
  return concat([u8(1), u64(value)]);
}

function requireAddress(bytes: Uint8Array): Uint8Array {
  if (bytes.length !== 20) throw new Error("address must be 20 bytes");
  return bytes;
}

function requireHash(bytes: Uint8Array): Uint8Array {
  if (bytes.length !== 32) throw new Error("hash must be 32 bytes");
  return bytes;
}

function jsonBytes(bytes: Uint8Array): number[] {
  return Array.from(bytes);
}

function jsonAddress(bytes: Uint8Array): number[] {
  return jsonBytes(requireAddress(bytes));
}

/**
 * Network-bound preimage: Borsh domain (`&[u8]`) + chain id + raw genesis + body.
 * `genesis` is `[u8; 32]` on the wire (no length prefix).
 */
export function encodeBoundEnvelope(
  domain: Uint8Array,
  chainId: string,
  genesisHex: string,
  body: Uint8Array,
): Uint8Array {
  const genesis = requireHash(hexToBytes(genesisHex));
  return concat([encodeVec(domain), encodeString(chainId), genesis, body]);
}

export function encodeAccountTransferBody(fields: {
  version: number;
  asset: number;
  from: Uint8Array;
  to: Uint8Array;
  amount: number;
  fee: number;
  nonce: number | bigint;
}): Uint8Array {
  return concat([
    u32(fields.version),
    u8(fields.asset),
    requireAddress(fields.from),
    requireAddress(fields.to),
    u64(fields.amount),
    u64(fields.fee),
    u64(fields.nonce),
  ]);
}

export function encodeOvlExecutionBody(fields: {
  version: number;
  from: Uint8Array;
  to: Uint8Array;
  value: number;
  gasLimit: number | bigint;
  maxFeePerGas: number | bigint;
  nonce: number | bigint;
  data: Uint8Array;
}): Uint8Array {
  return concat([
    u32(fields.version),
    requireAddress(fields.from),
    requireAddress(fields.to),
    u64(fields.value),
    u64(fields.gasLimit),
    u64(fields.maxFeePerGas),
    u64(fields.nonce),
    encodeVec(fields.data),
  ]);
}

export function encodeDrcPaymentV4Body(fields: {
  version: number;
  from: Uint8Array;
  to: Uint8Array;
  amount: number;
  fee: number;
  destinationTag?: number | null;
  sourceTag?: number | null;
  invoiceId: Uint8Array;
  nonce: number | bigint;
  lastValidBlueScore?: number | bigint | null;
}): Uint8Array {
  return concat([
    u32(fields.version),
    requireAddress(fields.from),
    requireAddress(fields.to),
    u64(fields.amount),
    u64(fields.fee),
    encodeOptionU32(fields.destinationTag),
    encodeOptionU32(fields.sourceTag),
    requireHash(fields.invoiceId),
    u64(fields.nonce),
    encodeOptionU64(fields.lastValidBlueScore),
  ]);
}

export type DrcBookAssetWire =
  | { type: "native_drc" }
  | { type: "issued"; issuer: Uint8Array; currency: Uint8Array };

function encodeBookAsset(asset: DrcBookAssetWire): Uint8Array {
  if (asset.type === "native_drc") return u8(DRC_BOOK_NATIVE);
  return concat([
    u8(DRC_BOOK_ISSUED),
    requireAddress(asset.issuer),
    requireAddress(asset.currency),
  ]);
}

export function encodeDrcOfferCreateBody(fields: {
  version: number;
  owner: Uint8Array;
  takerPays: DrcBookAssetWire;
  takerPaysAmount: number | bigint;
  takerGets: DrcBookAssetWire;
  takerGetsAmount: number | bigint;
  fillMode: number;
  timeInForce: number;
  fee: number;
  expiresAfterBlueScore?: number | bigint | null;
  nonce: number | bigint;
}): Uint8Array {
  return concat([
    encodeVec(DRC_OFFER_CREATE_TX_TYPE),
    u32(fields.version),
    requireAddress(fields.owner),
    encodeBookAsset(fields.takerPays),
    u64(fields.takerPaysAmount),
    encodeBookAsset(fields.takerGets),
    u64(fields.takerGetsAmount),
    u8(fields.fillMode),
    u8(fields.timeInForce),
    u64(fields.fee),
    encodeOptionU64(fields.expiresAfterBlueScore),
    u64(fields.nonce),
  ]);
}

export function encodeDrcOfferCancelBody(fields: {
  version: number;
  submitter: Uint8Array;
  offerId: Uint8Array;
  fee: number;
  nonce: number | bigint;
}): Uint8Array {
  return concat([
    encodeVec(DRC_OFFER_CANCEL_TX_TYPE),
    u32(fields.version),
    requireAddress(fields.submitter),
    requireHash(fields.offerId),
    u64(fields.fee),
    u64(fields.nonce),
  ]);
}

export function scriptP2pkh(address: Uint8Array): Uint8Array {
  requireAddress(address);
  return concat([
    Uint8Array.of(OP_DUP, OP_PUBKEYHASH, OP_PUSH, 20),
    address,
    Uint8Array.of(OP_EQUALVERIFY, OP_CHECKSIG),
  ]);
}

export function pushScriptData(data: Uint8Array): Uint8Array {
  if (data.length === 0) return Uint8Array.of(OP_0);
  if (data.length === 1 && data[0] >= 1 && data[0] <= 16) {
    return Uint8Array.of(OP_1 + data[0] - 1);
  }
  if (data.length > 80) throw new Error("script push too large");
  return concat([Uint8Array.of(OP_PUSH, data.length), data]);
}

export function encodeCovenantSighash(fields: {
  version: number;
  inputs: { txId: Uint8Array; index: number; sequence: number }[];
  outputs: { value: number; scriptPubkey: Uint8Array }[];
  lockTime: number | bigint;
  nonce: number | bigint;
}): Uint8Array {
  const inputParts = [u32(fields.inputs.length)];
  for (const input of fields.inputs) {
    inputParts.push(requireHash(input.txId), u32(input.index), u32(input.sequence));
  }
  const outputParts = [u32(fields.outputs.length)];
  for (const output of fields.outputs) {
    outputParts.push(u64(output.value), encodeVec(output.scriptPubkey));
  }
  return concat([
    encodeVec(COVENANT_SIGHASH_DOMAIN),
    u32(fields.version),
    concat(inputParts),
    concat(outputParts),
    u64(fields.lockTime),
    u64(fields.nonce),
  ]);
}

export function encodeCovenantSighashBound(
  chainId: string,
  genesisHex: string,
  sighash: Uint8Array,
): Uint8Array {
  const genesis = requireHash(hexToBytes(genesisHex));
  // Outer domain is `&[u8; N]` (no Borsh length prefix).
  return concat([
    COVENANT_BOUND_DOMAIN,
    encodeString(chainId),
    genesis,
    encodeVec(sighash),
  ]);
}

export function standardIssuedCurrency(code: string): Uint8Array {
  const trimmed = code.trim().toUpperCase();
  if (!/^[A-Z0-9]{3}$/.test(trimmed)) {
    throw new Error("issued currency must be 3 ASCII uppercase/digit bytes");
  }
  if (["DRC", "TLT", "OVL", "XRP"].includes(trimmed)) {
    throw new Error("reserved native-colliding currency code");
  }
  const out = new Uint8Array(20);
  out[0] = trimmed.charCodeAt(0);
  out[1] = trimmed.charCodeAt(1);
  out[2] = trimmed.charCodeAt(2);
  return out;
}

function jsonBookAsset(asset: DrcBookAssetWire): Record<string, unknown> {
  if (asset.type === "native_drc") return { type: "native_drc" };
  return {
    type: "issued",
    issuer: jsonAddress(asset.issuer),
    currency: jsonBytes(requireAddress(asset.currency)),
  };
}

async function signBound(
  secretKey: Uint8Array,
  domain: Uint8Array,
  chainId: string,
  genesisHex: string,
  body: Uint8Array,
): Promise<{ publicKey: Uint8Array; signature: Uint8Array; signingBytes: Uint8Array }> {
  const signingBytes = encodeBoundEnvelope(domain, chainId, genesisHex, body);
  const { publicKey, signature } = await signTransactionBody(secretKey, signingBytes);
  return { publicKey, signature, signingBytes };
}

function accountFromMnemonic(
  mnemonic: string,
  network: string,
  accountIndex: number,
): WalletAccount {
  return deriveAccount(mnemonic, accountIndex, "", network, 0);
}

function parseRecipient(input: string, network: string): { hex: string; bytes: Uint8Array } {
  const hex = parseAddress(input, network);
  return { hex, bytes: hexToBytes(hex) };
}

export async function buildSignedAccountTransfer(options: {
  mnemonic: string;
  accountIndex?: number;
  asset: NativeAssetTicker;
  toAddress: string;
  amount: number;
  fee: number;
  nonce: number;
  network?: string;
  genesisHash: string;
  chainId?: string;
}): Promise<BuiltTypedEnvelope> {
  if (options.amount <= 0) throw new Error("amount must be > 0");
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(options.mnemonic, network, options.accountIndex ?? 0);
  const to = parseRecipient(options.toAddress, network);
  const asset = options.asset === "OVL" ? NATIVE_ASSET_OVL : NATIVE_ASSET_DRC;
  const from = hexToBytes(account.addressHex);
  const body = encodeAccountTransferBody({
    version: ACCOUNT_TRANSFER_VERSION,
    asset,
    from,
    to: to.bytes,
    amount: options.amount,
    fee: options.fee,
    nonce: options.nonce,
  });
  const chainId = options.chainId ?? chainIdForNetwork(network);
  const { publicKey, signature, signingBytes } = await signBound(
    account.secretKey,
    ACCOUNT_TX_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return {
    tx: {
      version: ACCOUNT_TRANSFER_VERSION,
      asset: options.asset,
      from: jsonAddress(from),
      to: jsonAddress(to.bytes),
      amount: options.amount,
      fee: options.fee,
      nonce: options.nonce,
      public_key: jsonBytes(publicKey),
      signature: jsonBytes(signature),
    },
    from: account.addressHex,
    fromBech32: account.addressBech32,
    to: to.hex,
    toBech32: encodeAddress(to.hex, addressHrpForNetwork(network)),
    amount: options.amount,
    fee: options.fee,
    signingBytes,
  };
}

export async function buildSignedOvlExecution(options: {
  mnemonic: string;
  accountIndex?: number;
  toAddress: string;
  value: number;
  feePerGas?: number;
  gasLimit?: number;
  nonce: number;
  data?: Uint8Array;
  network?: string;
  genesisHash: string;
  chainId?: string;
}): Promise<BuiltTypedEnvelope> {
  if (options.data && options.data.length > 0) {
    throw new Error("OVL execution v1 keeps calldata empty; use the raw-EVM lane for bytecode");
  }
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(options.mnemonic, network, options.accountIndex ?? 0);
  const to = parseRecipient(options.toAddress, network);
  const from = hexToBytes(account.addressHex);
  const gasLimit = options.gasLimit ?? OVL_INTRINSIC_GAS;
  const maxFeePerGas = options.feePerGas ?? 1;
  const body = encodeOvlExecutionBody({
    version: OVL_EXECUTION_VERSION,
    from,
    to: to.bytes,
    value: options.value,
    gasLimit,
    maxFeePerGas,
    nonce: options.nonce,
    data: new Uint8Array(),
  });
  const chainId = options.chainId ?? chainIdForNetwork(network);
  const { publicKey, signature, signingBytes } = await signBound(
    account.secretKey,
    OVL_EXECUTION_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return {
    tx: {
      version: OVL_EXECUTION_VERSION,
      from: jsonAddress(from),
      to: jsonAddress(to.bytes),
      value: options.value,
      gas_limit: gasLimit,
      max_fee_per_gas: maxFeePerGas,
      nonce: options.nonce,
      data: [],
      public_key: jsonBytes(publicKey),
      signature: jsonBytes(signature),
    },
    from: account.addressHex,
    fromBech32: account.addressBech32,
    to: to.hex,
    toBech32: encodeAddress(to.hex, addressHrpForNetwork(network)),
    amount: options.value,
    fee: maxFeePerGas * gasLimit,
    signingBytes,
  };
}

export async function buildSignedDrcPayment(options: {
  mnemonic: string;
  accountIndex?: number;
  toAddress: string;
  amount: number;
  fee: number;
  nonce: number;
  destinationTag?: number | null;
  sourceTag?: number | null;
  invoiceId?: string;
  lastValidBlueScore?: number | null;
  network?: string;
  genesisHash: string;
  chainId?: string;
}): Promise<BuiltTypedEnvelope> {
  if (options.amount <= 0) throw new Error("amount must be > 0");
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(options.mnemonic, network, options.accountIndex ?? 0);
  const to = parseRecipient(options.toAddress, network);
  const from = hexToBytes(account.addressHex);
  const invoiceId = options.invoiceId ? hexToBytes(options.invoiceId) : new Uint8Array(32);
  const body = encodeDrcPaymentV4Body({
    version: DRC_PAYMENT_VERSION,
    from,
    to: to.bytes,
    amount: options.amount,
    fee: options.fee,
    destinationTag: options.destinationTag,
    sourceTag: options.sourceTag,
    invoiceId,
    nonce: options.nonce,
    lastValidBlueScore: options.lastValidBlueScore,
  });
  const chainId = options.chainId ?? chainIdForNetwork(network);
  const { publicKey, signature, signingBytes } = await signBound(
    account.secretKey,
    DRC_PAYMENT_V4_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return {
    tx: {
      version: DRC_PAYMENT_VERSION,
      from: jsonAddress(from),
      to: jsonAddress(to.bytes),
      amount: options.amount,
      fee: options.fee,
      destination_tag: options.destinationTag ?? null,
      source_tag: options.sourceTag ?? null,
      invoice_id: jsonBytes(invoiceId),
      nonce: options.nonce,
      last_valid_blue_score: options.lastValidBlueScore ?? null,
      public_key: jsonBytes(publicKey),
      signature: jsonBytes(signature),
    },
    from: account.addressHex,
    fromBech32: account.addressBech32,
    to: to.hex,
    toBech32: encodeAddress(to.hex, addressHrpForNetwork(network)),
    amount: options.amount,
    fee: options.fee,
    signingBytes,
  };
}

export async function buildSignedDrcOfferCreate(options: {
  mnemonic: string;
  accountIndex?: number;
  takerPaysAmount: number;
  takerGetsAmount: number;
  issuer: string;
  currency: string;
  fee: number;
  nonce: number;
  fillMode?: number;
  timeInForce?: number;
  expiresAfterBlueScore?: number | null;
  network?: string;
  genesisHash: string;
  chainId?: string;
}): Promise<BuiltTypedEnvelope> {
  if (options.takerPaysAmount <= 0 || options.takerGetsAmount <= 0) {
    throw new Error("offer amounts must be > 0");
  }
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(options.mnemonic, network, options.accountIndex ?? 0);
  const owner = hexToBytes(account.addressHex);
  const issuer = hexToBytes(parseAddress(options.issuer, network));
  const currency = standardIssuedCurrency(options.currency);
  const takerPays: DrcBookAssetWire = { type: "native_drc" };
  const takerGets: DrcBookAssetWire = { type: "issued", issuer, currency };
  const fillMode = options.fillMode ?? DRC_OFFER_FILL_BUY;
  const timeInForce = options.timeInForce ?? DRC_OFFER_TIF_GTC;
  const body = encodeDrcOfferCreateBody({
    version: DRC_OFFER_CREATE_TX_VERSION,
    owner,
    takerPays,
    takerPaysAmount: options.takerPaysAmount,
    takerGets,
    takerGetsAmount: options.takerGetsAmount,
    fillMode,
    timeInForce,
    fee: options.fee,
    expiresAfterBlueScore: options.expiresAfterBlueScore,
    nonce: options.nonce,
  });
  const chainId = options.chainId ?? chainIdForNetwork(network);
  const { publicKey, signature, signingBytes } = await signBound(
    account.secretKey,
    DRC_OFFER_CREATE_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return {
    tx: {
      version: DRC_OFFER_CREATE_TX_VERSION,
      owner: jsonAddress(owner),
      taker_pays: jsonBookAsset(takerPays),
      taker_pays_amount: options.takerPaysAmount,
      taker_gets: jsonBookAsset(takerGets),
      taker_gets_amount: options.takerGetsAmount,
      fill_mode: fillMode,
      time_in_force: timeInForce,
      fee: options.fee,
      expires_after_blue_score: options.expiresAfterBlueScore ?? null,
      nonce: options.nonce,
      public_key: jsonBytes(publicKey),
      signature: jsonBytes(signature),
    },
    from: account.addressHex,
    fromBech32: account.addressBech32,
    to: parseAddress(options.issuer, network),
    toBech32: encodeAddress(
      parseAddress(options.issuer, network),
      addressHrpForNetwork(network),
    ),
    amount: options.takerPaysAmount,
    fee: options.fee,
    signingBytes,
  };
}

export async function buildSignedDrcOfferCancel(options: {
  mnemonic: string;
  accountIndex?: number;
  offerId: string;
  fee: number;
  nonce: number;
  network?: string;
  genesisHash: string;
  chainId?: string;
}): Promise<BuiltTypedEnvelope> {
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(options.mnemonic, network, options.accountIndex ?? 0);
  const submitter = hexToBytes(account.addressHex);
  const offerId = hexToBytes(options.offerId);
  const body = encodeDrcOfferCancelBody({
    version: DRC_OFFER_CANCEL_TX_VERSION,
    submitter,
    offerId,
    fee: options.fee,
    nonce: options.nonce,
  });
  const chainId = options.chainId ?? chainIdForNetwork(network);
  const { publicKey, signature, signingBytes } = await signBound(
    account.secretKey,
    DRC_OFFER_CANCEL_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return {
    tx: {
      version: DRC_OFFER_CANCEL_TX_VERSION,
      submitter: jsonAddress(submitter),
      offer_id: jsonBytes(offerId),
      fee: options.fee,
      nonce: options.nonce,
      public_key: jsonBytes(publicKey),
      signature: jsonBytes(signature),
    },
    from: account.addressHex,
    fromBech32: account.addressBech32,
    to: bytesToHex(offerId).slice(0, 40),
    toBech32: account.addressBech32,
    amount: 0,
    fee: options.fee,
    signingBytes,
  };
}

export async function buildSignedTltCovenant(options: {
  mnemonic: string;
  accountIndex?: number;
  utxos: LightUtxo[];
  toAddress: string;
  amount: number;
  fee: number;
  nonce?: number;
  network?: string;
  genesisHash: string;
  chainId?: string;
  spendChange?: boolean;
}): Promise<BuiltTypedEnvelope> {
  const network = options.network ?? "mainnet";
  const spendChange = options.spendChange === true;
  const account = deriveAccount(
    options.mnemonic,
    options.accountIndex ?? 0,
    "",
    network,
    spendChange ? 1 : 0,
  );
  const changeAccount = deriveAccount(
    options.mnemonic,
    options.accountIndex ?? 0,
    "",
    network,
    1,
  );
  const need = options.amount + options.fee;
  if (need <= 0) throw new Error("amount must be > 0");
  const to = parseRecipient(options.toAddress, network);
  const selected = selectTltCoins(options.utxos, options.amount, options.fee);
  const totalIn = selected.reduce((sum, utxo) => sum + utxo.value, 0);
  const change = totalIn - need;
  const outputs: { value: number; scriptPubkey: Uint8Array }[] = [
    { value: options.amount, scriptPubkey: scriptP2pkh(to.bytes) },
  ];
  if (change > 0) {
    outputs.push({
      value: change,
      scriptPubkey: scriptP2pkh(hexToBytes(changeAccount.addressHex)),
    });
  }
  const inputs = selected.map((utxo) => ({
    txId: hexToBytes(utxo.tx_id),
    index: utxo.index,
    sequence: TLT_SEQUENCE_FINAL,
  }));
  const nonce = options.nonce ?? Date.now();
  const sighash = encodeCovenantSighash({
    version: TLT_COVENANT_TX_VERSION,
    inputs,
    outputs,
    lockTime: 0,
    nonce,
  });
  const chainId = options.chainId ?? chainIdForNetwork(network);
  const signingBytes = encodeCovenantSighashBound(chainId, options.genesisHash, sighash);
  const { publicKey, signature } = await signTransactionBody(account.secretKey, signingBytes);
  const scriptSig = concat([pushScriptData(signature), pushScriptData(publicKey)]);
  return {
    tx: {
      version: TLT_COVENANT_TX_VERSION,
      inputs: selected.map((utxo) => ({
        previous_outpoint: {
          tx_id: jsonBytes(hexToBytes(utxo.tx_id)),
          index: utxo.index,
        },
        sequence: TLT_SEQUENCE_FINAL,
        script_sig: jsonBytes(scriptSig),
      })),
      outputs: outputs.map((output) => ({
        value: output.value,
        script_pubkey: jsonBytes(output.scriptPubkey),
      })),
      lock_time: 0,
      nonce,
    },
    from: account.addressHex,
    fromBech32: account.addressBech32,
    to: to.hex,
    toBech32: encodeAddress(to.hex, addressHrpForNetwork(network)),
    amount: options.amount,
    fee: options.fee,
    signingBytes,
  };
}

async function nodeBinding(client: LightClient, network: string): Promise<{
  genesisHash: string;
  chainId: string;
  network: string;
}> {
  const info = await client.getNodeInfo();
  const genesisHash = info.genesis_hash;
  if (!genesisHash) throw new Error("agora_getNodeInfo did not return genesis_hash");
  return {
    genesisHash,
    chainId: info.chain_id ?? chainIdForNetwork(info.network || network),
    network: info.network || network,
  };
}

async function accountNonce(
  client: LightClient,
  addressHex: string,
  asset: NativeAssetTicker,
): Promise<number> {
  const balances = await client.getAccountBalances(addressHex);
  return asset === "OVL" ? balances.ovl.nonce : balances.drc.nonce;
}

export async function sendAccountTransfer(
  client: LightClient,
  options: {
    mnemonic: string;
    asset: NativeAssetTicker;
    toAddress: string;
    amount: number;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
): Promise<{ id: string; built: BuiltTypedEnvelope }> {
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(options.mnemonic, network, options.accountIndex ?? 0);
  const bind = await nodeBinding(client, network);
  let fee = options.fee;
  if (fee === undefined) {
    try {
      fee = (await client.estimateFee()).suggested_fee;
    } catch {
      fee = 1;
    }
  }
  const nonce = await accountNonce(client, account.addressHex, options.asset);
  const built = await buildSignedAccountTransfer({
    ...options,
    fee,
    nonce,
    genesisHash: bind.genesisHash,
    chainId: bind.chainId,
    network: bind.network,
  });
  const result = await client.submitAccountTransfer(built.tx);
  return { id: result.account_tx_id, built };
}

export async function sendOvlExecution(
  client: LightClient,
  options: {
    mnemonic: string;
    toAddress: string;
    value: number;
    feePerGas?: number;
    network?: string;
    accountIndex?: number;
  },
): Promise<{ id: string; built: BuiltTypedEnvelope }> {
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(options.mnemonic, network, options.accountIndex ?? 0);
  const bind = await nodeBinding(client, network);
  const nonce = await accountNonce(client, account.addressHex, "OVL");
  const built = await buildSignedOvlExecution({
    ...options,
    nonce,
    genesisHash: bind.genesisHash,
    chainId: bind.chainId,
    network: bind.network,
  });
  const result = await client.submitOvlExecution(built.tx);
  return { id: result.execution_tx_id, built };
}

export async function sendDrcPayment(
  client: LightClient,
  options: {
    mnemonic: string;
    toAddress: string;
    amount: number;
    fee?: number;
    network?: string;
    accountIndex?: number;
    destinationTag?: number | null;
    sourceTag?: number | null;
  },
): Promise<{ id: string; built: BuiltTypedEnvelope }> {
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(options.mnemonic, network, options.accountIndex ?? 0);
  const bind = await nodeBinding(client, network);
  let fee = options.fee;
  if (fee === undefined) {
    try {
      fee = (await client.estimateFee()).suggested_fee;
    } catch {
      fee = 1;
    }
  }
  const nonce = await accountNonce(client, account.addressHex, "DRC");
  const built = await buildSignedDrcPayment({
    ...options,
    fee,
    nonce,
    genesisHash: bind.genesisHash,
    chainId: bind.chainId,
    network: bind.network,
  });
  const result = await client.submitDrcPayment(built.tx);
  return { id: result.payment_id, built };
}

export async function sendDrcOfferCreate(
  client: LightClient,
  options: {
    mnemonic: string;
    takerPaysAmount: number;
    takerGetsAmount: number;
    issuer: string;
    currency: string;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
): Promise<{ id: string; built: BuiltTypedEnvelope }> {
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(options.mnemonic, network, options.accountIndex ?? 0);
  const bind = await nodeBinding(client, network);
  let fee = options.fee;
  if (fee === undefined) {
    try {
      fee = (await client.estimateFee()).suggested_fee;
    } catch {
      fee = 1;
    }
  }
  const nonce = await accountNonce(client, account.addressHex, "DRC");
  const built = await buildSignedDrcOfferCreate({
    ...options,
    fee,
    nonce,
    genesisHash: bind.genesisHash,
    chainId: bind.chainId,
    network: bind.network,
  });
  const result = await client.submitDrcOfferCreate(built.tx);
  return { id: result.offer_id, built };
}

export async function sendDrcOfferCancel(
  client: LightClient,
  options: {
    mnemonic: string;
    offerId: string;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
): Promise<{ id: string; built: BuiltTypedEnvelope }> {
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(options.mnemonic, network, options.accountIndex ?? 0);
  const bind = await nodeBinding(client, network);
  let fee = options.fee;
  if (fee === undefined) {
    try {
      fee = (await client.estimateFee()).suggested_fee;
    } catch {
      fee = 1;
    }
  }
  const nonce = await accountNonce(client, account.addressHex, "DRC");
  const built = await buildSignedDrcOfferCancel({
    ...options,
    fee,
    nonce,
    genesisHash: bind.genesisHash,
    chainId: bind.chainId,
    network: bind.network,
  });
  const result = await client.submitDrcOfferCancel(built.tx);
  return { id: result.cancel_tx_id, built };
}

export async function sendTltCovenant(
  client: LightClient,
  options: {
    mnemonic: string;
    toAddress: string;
    amount: number;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
): Promise<{ id: string; built: BuiltTypedEnvelope }> {
  const accountIndex = options.accountIndex ?? 0;
  const network = options.network ?? "mainnet";
  let fee = options.fee;
  if (fee === undefined) {
    try {
      fee = (await client.estimateFee()).suggested_fee;
    } catch {
      fee = 1;
    }
  }
  const receive = deriveAccount(options.mnemonic, accountIndex, "", network, 0);
  const changeAcc = deriveAccount(options.mnemonic, accountIndex, "", network, 1);
  const [ext, ch] = await Promise.all([
    client.getUtxos(receive.addressHex),
    client.getUtxos(changeAcc.addressHex),
  ]);
  const need = options.amount + fee;
  const sum = (utxos: { value: number }[]) => utxos.reduce((a, b) => a + b.value, 0);
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
  const bind = await nodeBinding(client, network);
  const built = await buildSignedTltCovenant({
    ...options,
    fee,
    utxos,
    spendChange,
    genesisHash: bind.genesisHash,
    chainId: bind.chainId,
    network: bind.network,
  });
  const result = await client.submitTltCovenant(built.tx);
  return { id: result.tx_id, built };
}
