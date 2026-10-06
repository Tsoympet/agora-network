/**
 * Locked Borsh preimages matching agora-types light_client_vectors.
 * Run: `node --experimental-strip-types light-client/typed-lanes.test.ts`
 */
import assert from "node:assert/strict";

import {
  ACCOUNT_TRANSFER_VERSION,
  ACCOUNT_TX_SIGNING_DOMAIN,
  DRC_BOOK_ISSUED,
  DRC_OFFER_CANCEL_SIGNING_DOMAIN,
  DRC_OFFER_CREATE_SIGNING_DOMAIN,
  DRC_OFFER_CREATE_TX_VERSION,
  DRC_PAYMENT_V4_SIGNING_DOMAIN,
  DRC_PAYMENT_VERSION,
  NATIVE_ASSET_OVL,
  OVL_EXECUTION_SIGNING_DOMAIN,
  OVL_EXECUTION_VERSION,
  TLT_COVENANT_TX_VERSION,
  TLT_SEQUENCE_FINAL,
  bytesToHex,
  encodeAccountTransferBody,
  encodeBoundEnvelope,
  encodeCovenantSighash,
  encodeCovenantSighashBound,
  encodeDrcOfferCancelBody,
  encodeDrcOfferCreateBody,
  encodeDrcPaymentV4Body,
  encodeOvlExecutionBody,
  scriptP2pkh,
  standardIssuedCurrency,
} from "./typed-lanes.ts";

const GENESIS =
  "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const CHAIN = "agora-testnet-1";

function addr(fill: number): Uint8Array {
  return new Uint8Array(20).fill(fill);
}

function hash(fill: number): Uint8Array {
  return new Uint8Array(32).fill(fill);
}

const accountPreimage = encodeBoundEnvelope(
  ACCOUNT_TX_SIGNING_DOMAIN,
  CHAIN,
  GENESIS,
  encodeAccountTransferBody({
    version: ACCOUNT_TRANSFER_VERSION,
    asset: NATIVE_ASSET_OVL,
    from: addr(1),
    to: addr(2),
    amount: 10,
    fee: 1,
    nonce: 7,
  }),
);
assert.equal(
  bytesToHex(accountPreimage),
  "1b00000061676f72612d74726964656e742d6163636f756e742d74782d76320f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0200000001010101010101010101010101010101010101010102020202020202020202020202020202020202020a0000000000000001000000000000000700000000000000",
);

const executionPreimage = encodeBoundEnvelope(
  OVL_EXECUTION_SIGNING_DOMAIN,
  CHAIN,
  GENESIS,
  encodeOvlExecutionBody({
    version: OVL_EXECUTION_VERSION,
    from: addr(1),
    to: addr(2),
    value: 3,
    gasLimit: 21_000,
    maxFeePerGas: 1,
    nonce: 4,
    data: new Uint8Array(),
  }),
);
assert.equal(
  bytesToHex(executionPreimage),
  "1e00000061676f72612d74726964656e742d6f766c2d657865637574696f6e2d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0100000001010101010101010101010101010101010101010202020202020202020202020202020202020202030000000000000008520000000000000100000000000000040000000000000000000000",
);

const paymentPreimage = encodeBoundEnvelope(
  DRC_PAYMENT_V4_SIGNING_DOMAIN,
  CHAIN,
  GENESIS,
  encodeDrcPaymentV4Body({
    version: DRC_PAYMENT_VERSION,
    from: addr(1),
    to: addr(2),
    amount: 9,
    fee: 1,
    invoiceId: new Uint8Array(32),
    nonce: 5,
  }),
);
assert.equal(
  bytesToHex(paymentPreimage),
  "1c00000061676f72612d74726964656e742d6472632d7061796d656e742d76340f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef04000000010101010101010101010101010101010101010102020202020202020202020202020202020202020900000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000050000000000000000",
);

const currency = standardIssuedCurrency("USD");
assert.equal(currency[0], 0x55);
assert.equal(currency[1], 0x53);
assert.equal(currency[2], 0x44);
assert.throws(() => standardIssuedCurrency("DRC"), /reserved/);

const offerCreatePreimage = encodeBoundEnvelope(
  DRC_OFFER_CREATE_SIGNING_DOMAIN,
  CHAIN,
  GENESIS,
  encodeDrcOfferCreateBody({
    version: DRC_OFFER_CREATE_TX_VERSION,
    owner: addr(1),
    takerPays: { type: "native_drc" },
    takerPaysAmount: 8,
    takerGets: { type: "issued", issuer: addr(2), currency },
    takerGetsAmount: 4,
    fillMode: 1,
    timeInForce: 1,
    fee: 1,
    nonce: 3,
  }),
);
assert.equal(
  bytesToHex(offerCreatePreimage),
  "2100000061676f72612d74726964656e742d6472632d6f666665722d6372656174652d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef100000006472635f6f666665725f6372656174650100000001010101010101010101010101010101010101010108000000000000000202020202020202020202020202020202020202025553440000000000000000000000000000000000040000000000000001010100000000000000000300000000000000",
);
assert.equal(DRC_BOOK_ISSUED, 2);

const offerCancelPreimage = encodeBoundEnvelope(
  DRC_OFFER_CANCEL_SIGNING_DOMAIN,
  CHAIN,
  GENESIS,
  encodeDrcOfferCancelBody({
    version: 1,
    submitter: addr(1),
    offerId: hash(9),
    fee: 1,
    nonce: 6,
  }),
);
assert.equal(
  bytesToHex(offerCancelPreimage),
  "2100000061676f72612d74726964656e742d6472632d6f666665722d63616e63656c2d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef100000006472635f6f666665725f63616e63656c010000000101010101010101010101010101010101010101090909090909090909090909090909090909090909090909090909090909090901000000000000000600000000000000",
);

const covenantSighash = encodeCovenantSighash({
  version: TLT_COVENANT_TX_VERSION,
  inputs: [{ txId: hash(4), index: 0, sequence: TLT_SEQUENCE_FINAL }],
  outputs: [{ value: 1, scriptPubkey: scriptP2pkh(addr(1)) }],
  lockTime: 0,
  nonce: 9,
});
const covenantBound = encodeCovenantSighashBound(CHAIN, GENESIS, covenantSighash);
assert.equal(
  bytesToHex(covenantBound),
  "61676f72612d746c742d636f76656e616e742d736967686173682d626f756e642d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef8b0000001d00000061676f72612d746c742d636f76656e616e742d736967686173682d76310200000001000000040404040404040404040404040404040404040404040404040404040404040400000000ffffffff0100000001000000000000001a00000076a90114010101010101010101010101010101010101010188ac00000000000000000900000000000000",
);

console.log("typed-lane Borsh preimages match agora-types");
