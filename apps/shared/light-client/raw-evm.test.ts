/**
 * Locked raw OVL-EVM envelopes matching agora-ovl-evm sign_legacy / sign_eip1559.
 * Run: `node --experimental-strip-types light-client/raw-evm.test.ts`
 */
import assert from "node:assert/strict";

import {
  RAW_EVM_DEV_CHAIN_ID,
  RAW_EVM_KEY_SPLIT,
  ethereumAddressFromSecret,
  signEip1559RawTransaction,
  signLegacyRawTransaction,
} from "./raw-evm.ts";

const DEV_KEY = "11".repeat(32);
const TO = "22".repeat(20);

assert.equal(
  ethereumAddressFromSecret(DEV_KEY),
  "19e7e376e7c213b7e7e7e46cc70a5dd086daff2a",
);
assert.match(RAW_EVM_KEY_SPLIT.agoraVault, /BIP-44/);
assert.match(RAW_EVM_KEY_SPLIT.rawEvm, /explicit 32-byte/);

const legacy = await signLegacyRawTransaction(DEV_KEY, {
  chainId: RAW_EVM_DEV_CHAIN_ID,
  nonce: 3,
  gasPrice: 2,
  gasLimit: 21_000,
  to: TO,
});
assert.equal(
  legacy,
  "f8620302825208942222222222222222222222222222222222222222808083024243a0e5a1285a76a3c2748c5218cf70425d5baaf6fcd98937b0564e531f37d2f55b7ea05b9dd476c30c8f668ef932b8ee33db9e0ae3579b2f5959895676eb997e285cf0",
);

const eip1559 = await signEip1559RawTransaction(DEV_KEY, {
  chainId: RAW_EVM_DEV_CHAIN_ID,
  nonce: 4,
  maxPriorityFeePerGas: 1,
  maxFeePerGas: 2,
  gasLimit: 21_000,
  to: TO,
  data: Uint8Array.of(0x01),
});
assert.equal(
  eip1559,
  "02f865830121100401028252089422222222222222222222222222222222222222228001c080a06bcbec0ea4d0d685339e55f72721115589e2e8bf99300422018aa727eb059f93a01fc26256b1d7347f0d31b6fa1bd7ebb937d2f921bfa8d404fb15608ea134284a",
);

console.log("raw-evm helper matches agora-ovl-evm locked envelopes");
