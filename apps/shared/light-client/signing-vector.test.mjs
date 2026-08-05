/**
 * Cross-language signing vector smoke (mirrors agora-crypto::bound_signing_vector).
 * Run from apps/shared: `node light-client/signing-vector.test.mjs`
 */
import { sha256 } from "@noble/hashes/sha256";
import * as secp from "@noble/secp256k1";
import { HDKey } from "@scure/bip32";
import { mnemonicToSeedSync } from "@scure/bip39";

const PHRASE =
  "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const GENESIS =
  "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const EXPECT_PREIMAGE =
  "0b00000061676f72612d74782d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef010000000100000000000000000000000000000000000000000000000000000000000000000000000000000001000000e803000000000000ff9ec96f09eb154d038a552ecae59c50204ea9a92a00000000000000";
const EXPECT_PUB =
  "03ae62ade894b15c2b7aa2c61ac1103ee2de672f93668ab05a2760060d7f59b397";
const EXPECT_SIG =
  "7c66bbaf11a82cf00f8edafd90e6c99627f4b4d25e1e285d7eeeb8f8dac01053354eb9b3b206246391bd61fa545af1d4b61631d5a7d2e025ea239b63a511a4a2";

const TX_SIGNING_DOMAIN = new TextEncoder().encode("agora-tx-v1");

function hexToBytes(hex) {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}
function bytesToHex(bytes) {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}
function writeU32(view, offset, value) {
  view.setUint32(offset, value >>> 0, true);
  return offset + 4;
}
function writeU64(view, offset, value) {
  view.setBigUint64(offset, BigInt(value), true);
  return offset + 8;
}

function encodeTransactionBody(body) {
  let size = 4 + 4 + 4 + 8;
  for (const _ of body.inputs) size += 32 + 4;
  for (const _ of body.outputs) size += 8 + 20;
  const buf = new ArrayBuffer(size);
  const view = new DataView(buf);
  const bytes = new Uint8Array(buf);
  let o = 0;
  o = writeU32(view, o, body.version);
  o = writeU32(view, o, body.inputs.length);
  for (const input of body.inputs) {
    bytes.set(input.tx_id, o);
    o += 32;
    o = writeU32(view, o, input.index);
  }
  o = writeU32(view, o, body.outputs.length);
  for (const output of body.outputs) {
    o = writeU64(view, o, output.value);
    bytes.set(output.address, o);
    o += 20;
  }
  writeU64(view, o, body.nonce);
  return bytes;
}

function encodeSigningBytesBound(chainId, genesisHex, body) {
  const genesis = hexToBytes(genesisHex);
  const chainBytes = new TextEncoder().encode(chainId);
  const bodyBytes = encodeTransactionBody(body);
  const size =
    4 + TX_SIGNING_DOMAIN.length + 4 + chainBytes.length + 32 + bodyBytes.length;
  const buf = new ArrayBuffer(size);
  const view = new DataView(buf);
  const bytes = new Uint8Array(buf);
  let o = 0;
  o = writeU32(view, o, TX_SIGNING_DOMAIN.length);
  bytes.set(TX_SIGNING_DOMAIN, o);
  o += TX_SIGNING_DOMAIN.length;
  o = writeU32(view, o, chainBytes.length);
  bytes.set(chainBytes, o);
  o += chainBytes.length;
  bytes.set(genesis, o);
  o += 32;
  bytes.set(bodyBytes, o);
  return bytes;
}

const seed = mnemonicToSeedSync(PHRASE, "");
const hd = HDKey.fromMasterSeed(seed).derive("m/44'/8888'/0'/0/0");
const secretKey = hd.privateKey;
const address = sha256(hd.publicKey).slice(0, 20);

const body = {
  version: 1,
  inputs: [{ tx_id: new Uint8Array(32), index: 0 }],
  outputs: [{ value: 1000, address }],
  nonce: 42,
};

const preimage = encodeSigningBytesBound("agora-testnet-1", GENESIS, body);
const preHex = bytesToHex(preimage);
if (preHex !== EXPECT_PREIMAGE) {
  throw new Error(`preimage mismatch\n got ${preHex}\nwant ${EXPECT_PREIMAGE}`);
}

const digest = sha256(preimage);
const signature = await secp.signAsync(digest, secretKey, { lowS: true });
const compact =
  signature instanceof Uint8Array
    ? signature.slice(0, 64)
    : signature.toCompactRawBytes();
const publicKey = secp.getPublicKey(secretKey, true);

if (bytesToHex(publicKey) !== EXPECT_PUB) {
  throw new Error(`pubkey mismatch ${bytesToHex(publicKey)}`);
}
if (bytesToHex(compact) !== EXPECT_SIG) {
  throw new Error(`sig mismatch ${bytesToHex(compact)}`);
}

console.log("bound signing vector OK (matches Rust agora-crypto)");
