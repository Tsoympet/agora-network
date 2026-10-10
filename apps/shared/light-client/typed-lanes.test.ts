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
import {
  DRC_ACCOUNT_POLICY_ACTIONS,
  DRC_WALLET_FAMILY_LANES,
  drcFamilyNeedsAmount,
  drcFamilyNeedsExtraHex,
  drcFamilyNeedsIssued,
  drcFamilyNeedsObjectId,
  drcFamilyNeedsPolicyAction,
  drcFamilyNeedsRecipient,
  drcFamilyNeedsSignerList,
  parseDrcSignerEntries,
} from "./typed-lanes-drc.ts";

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

import {
  DRC_ACCOUNT_POLICY_V1_SIGNING_DOMAIN,
  DRC_CHECK_CASH_SIGNING_DOMAIN,
  DRC_CHECK_CREATE_SIGNING_DOMAIN,
  DRC_CHANNEL_CREATE_SIGNING_DOMAIN,
  DRC_CHANNEL_FUND_SIGNING_DOMAIN,
  DRC_DEPOSIT_PREAUTH_SIGNING_DOMAIN,
  DRC_ESCROW_CREATE_SIGNING_DOMAIN,
  DRC_ESCROW_FINISH_SIGNING_DOMAIN,
  DRC_ISSUED_POLICY_SIGNING_DOMAIN,
  DRC_ISSUED_TRANSFER_SIGNING_DOMAIN,
  DRC_REGULAR_KEY_SIGNING_DOMAIN,
  DRC_SIGNER_LIST_SIGNING_DOMAIN,
  DRC_TICKET_CREATE_SIGNING_DOMAIN,
  DRC_TRUST_LINE_SET_SIGNING_DOMAIN,
  encodeDrcAccountPolicyV1Body,
  encodeDrcCheckCashBody,
  encodeDrcCheckCreateBody,
  encodeDrcChannelCreateBody,
  encodeDrcChannelFundBody,
  encodeDrcDepositPreauthBody,
  encodeDrcEscrowCreateBody,
  encodeDrcEscrowFinishBody,
  encodeDrcIssuedPolicyBody,
  encodeDrcIssuedTransferBody,
  encodeDrcRegularKeyBody,
  encodeDrcSignerListBody,
  encodeDrcTicketCreateBody,
  encodeDrcTrustLineSetBody,
} from "./typed-lanes-drc.ts";
import {
  encodePassportAttestationBody,
  PASSPORT_ATTESTATION_DOMAIN,
} from "./typed-lanes-passport.ts";
import {
  encodeGrantRegistrationBody,
  encodeHubRegistrationBody,
  encodeMissionRegistrationBody,
  GRANT_REGISTRATION_DOMAIN,
  HUB_REGISTRATION_DOMAIN,
  MISSION_REGISTRATION_DOMAIN,
} from "./typed-lanes-community.ts";
import {
  encodeTreasuryDisbursementBody,
  TREASURY_DISBURSEMENT_DOMAIN,
} from "./typed-lanes-treasury.ts";
import {
  encodeVestingUnlockBody,
  VESTING_UNLOCK_DOMAIN,
} from "./typed-lanes-vesting.ts";
import { sha256 } from "@noble/hashes/sha256";

function lock(name: string, got: Uint8Array, expected: string) {
  assert.equal(bytesToHex(got), expected, name);
}

lock(
  "escrow_create",
  encodeBoundEnvelope(
    DRC_ESCROW_CREATE_SIGNING_DOMAIN,
    CHAIN,
    GENESIS,
    encodeDrcEscrowCreateBody({
      version: 1,
      owner: addr(1),
      recipient: addr(2),
      amount: 9,
      fee: 1,
      invoiceId: new Uint8Array(32),
      nonce: 5,
    }),
  ),
  "2200000061676f72612d74726964656e742d6472632d657363726f772d6372656174652d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef110000006472635f657363726f775f6372656174650100000001010101010101010101010101010101010101010202020202020202020202020202020202020202090000000000000001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000500000000000000",
);
lock(
  "escrow_finish",
  encodeBoundEnvelope(
    DRC_ESCROW_FINISH_SIGNING_DOMAIN,
    CHAIN,
    GENESIS,
    encodeDrcEscrowFinishBody({
      version: 1,
      submitter: addr(1),
      escrowId: hash(9),
      fee: 1,
      nonce: 6,
    }),
  ),
  "2200000061676f72612d74726964656e742d6472632d657363726f772d66696e6973682d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef110000006472635f657363726f775f66696e697368010000000101010101010101010101010101010101010101090909090909090909090909090909090909090909090909090909090909090901000000000000000600000000000000",
);
lock(
  "ticket_create",
  encodeBoundEnvelope(
    DRC_TICKET_CREATE_SIGNING_DOMAIN,
    CHAIN,
    GENESIS,
    encodeDrcTicketCreateBody({ version: 1, owner: addr(1), nonce: 4, fee: 1 }),
  ),
  "2200000061676f72612d74726964656e742d6472632d7469636b65742d6372656174652d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef110000006472635f7469636b65745f63726561746501000000010101010101010101010101010101010101010104000000000000000100000000000000",
);
lock(
  "account_policy_v1",
  encodeBoundEnvelope(
    DRC_ACCOUNT_POLICY_V1_SIGNING_DOMAIN,
    CHAIN,
    GENESIS,
    encodeDrcAccountPolicyV1Body({
      version: 1,
      account: addr(1),
      action: 0,
      fee: 1,
      nonce: 3,
    }),
  ),
  "2300000061676f72612d74726964656e742d6472632d6163636f756e742d706f6c6963792d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0100000001010101010101010101010101010101010101010001000000000000000300000000000000",
);
lock(
  "deposit_preauth",
  encodeBoundEnvelope(
    DRC_DEPOSIT_PREAUTH_SIGNING_DOMAIN,
    CHAIN,
    GENESIS,
    encodeDrcDepositPreauthBody({
      version: 1,
      owner: addr(1),
      action: 0,
      authorizedSource: addr(2),
      nonce: 2,
      fee: 1,
    }),
  ),
  "2400000061676f72612d74726964656e742d6472632d6465706f7369742d707265617574682d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef130000006472635f6465706f7369745f7072656175746801000000010101010101010101010101010101010101010100020202020202020202020202020202020202020202000000000000000100000000000000",
);
lock(
  "regular_key_clear",
  encodeBoundEnvelope(
    DRC_REGULAR_KEY_SIGNING_DOMAIN,
    CHAIN,
    GENESIS,
    encodeDrcRegularKeyBody({
      version: 1,
      owner: addr(1),
      action: 1,
      regularKey: new Uint8Array(20),
      regularKeyPublicKey: new Uint8Array(),
      nonce: 8,
      fee: 1,
    }),
  ),
  "2000000061676f72612d74726964656e742d6472632d726567756c61722d6b65792d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0f0000006472635f726567756c61725f6b65790100000001010101010101010101010101010101010101010100000000000000000000000000000000000000000000000008000000000000000100000000000000",
);
lock(
  "signer_list_set",
  encodeBoundEnvelope(
    DRC_SIGNER_LIST_SIGNING_DOMAIN,
    CHAIN,
    GENESIS,
    encodeDrcSignerListBody({
      version: 1,
      owner: addr(1),
      action: 0,
      quorum: 1,
      entries: [{ signer: addr(2), weight: 1 }],
      nonce: 7,
      fee: 1,
    }),
  ),
  "2000000061676f72612d74726964656e742d6472632d7369676e65722d6c6973742d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0f0000006472635f7369676e65725f6c6973740100000001010101010101010101010101010101010101010001000000010000000202020202020202020202020202020202020202010007000000000000000100000000000000",
);
lock(
  "check_create",
  encodeBoundEnvelope(
    DRC_CHECK_CREATE_SIGNING_DOMAIN,
    CHAIN,
    GENESIS,
    encodeDrcCheckCreateBody({
      version: 1,
      owner: addr(1),
      destination: addr(2),
      amount: 9,
      fee: 1,
      invoiceId: new Uint8Array(32),
      nonce: 5,
    }),
  ),
  "2100000061676f72612d74726964656e742d6472632d636865636b2d6372656174652d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef100000006472635f636865636b5f63726561746501000000010101010101010101010101010101010101010102020202020202020202020202020202020202020900000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000500000000000000",
);
lock(
  "check_cash",
  encodeBoundEnvelope(
    DRC_CHECK_CASH_SIGNING_DOMAIN,
    CHAIN,
    GENESIS,
    encodeDrcCheckCashBody({
      version: 1,
      submitter: addr(1),
      checkId: hash(9),
      fee: 1,
      nonce: 6,
    }),
  ),
  "1f00000061676f72612d74726964656e742d6472632d636865636b2d636173682d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0e0000006472635f636865636b5f63617368010000000101010101010101010101010101010101010101090909090909090909090909090909090909090909090909090909090909090901000000000000000600000000000000",
);
lock(
  "channel_create",
  encodeBoundEnvelope(
    DRC_CHANNEL_CREATE_SIGNING_DOMAIN,
    CHAIN,
    GENESIS,
    encodeDrcChannelCreateBody({
      version: 1,
      owner: addr(1),
      destination: addr(2),
      amount: 9,
      fee: 1,
      claimPublicKey: new Uint8Array(33).fill(2),
      settleDelay: 4,
      invoiceId: new Uint8Array(32),
      nonce: 5,
    }),
  ),
  "2b00000061676f72612d74726964656e742d6472632d7061796d656e742d6368616e6e656c2d6372656174652d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef1a0000006472635f7061796d656e745f6368616e6e656c5f63726561746501000000010101010101010101010101010101010101010102020202020202020202020202020202020202020900000000000000010000000000000021000000020202020202020202020202020202020202020202020202020202020202020202040000000000000000000000000000000000000000000000000000000000000000000000000000000000000500000000000000",
);
lock(
  "channel_fund",
  encodeBoundEnvelope(
    DRC_CHANNEL_FUND_SIGNING_DOMAIN,
    CHAIN,
    GENESIS,
    encodeDrcChannelFundBody({
      version: 1,
      submitter: addr(1),
      channelId: hash(9),
      amount: 3,
      fee: 1,
      nonce: 6,
    }),
  ),
  "2900000061676f72612d74726964656e742d6472632d7061796d656e742d6368616e6e656c2d66756e642d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef180000006472635f7061796d656e745f6368616e6e656c5f66756e640100000001010101010101010101010101010101010101010909090909090909090909090909090909090909090909090909090909090909030000000000000001000000000000000600000000000000",
);
lock(
  "trust_line_set",
  encodeBoundEnvelope(
    DRC_TRUST_LINE_SET_SIGNING_DOMAIN,
    CHAIN,
    GENESIS,
    encodeDrcTrustLineSetBody({
      version: 1,
      holder: addr(1),
      issuer: addr(2),
      currency,
      limit: 100,
      fee: 1,
      nonce: 4,
    }),
  ),
  "2300000061676f72612d74726964656e742d6472632d74727573742d6c696e652d7365742d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0100000001010101010101010101010101010101010101010202020202020202020202020202020202020202555344000000000000000000000000000000000064000000000000000100000000000000040000000000000000",
);
lock(
  "issued_transfer",
  encodeBoundEnvelope(
    DRC_ISSUED_TRANSFER_SIGNING_DOMAIN,
    CHAIN,
    GENESIS,
    encodeDrcIssuedTransferBody({
      version: 1,
      sender: addr(1),
      recipient: addr(2),
      issuer: addr(3),
      currency,
      amount: 7,
      fee: 1,
      invoiceId: new Uint8Array(32),
      nonce: 5,
    }),
  ),
  "2400000061676f72612d74726964656e742d6472632d6973737565642d7472616e736665722d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0100000001010101010101010101010101010101010101010202020202020202020202020202020202020202030303030303030303030303030303030303030355534400000000000000000000000000000000000700000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000050000000000000000",
);
lock(
  "issued_policy_hash",
  sha256(
    encodeBoundEnvelope(
      DRC_ISSUED_POLICY_SIGNING_DOMAIN,
      CHAIN,
      GENESIS,
      encodeDrcIssuedPolicyBody({
        version: 1,
        issuer: addr(1),
        currency,
        action: 1,
        fee: 1,
        nonce: 4,
      }),
    ),
  ),
  "d2a02cf60231a28a6d807b91089da97d09d595a6af4bfb76c78b9155aadd1d21",
);
lock(
  "passport_attestation",
  encodeBoundEnvelope(
    PASSPORT_ATTESTATION_DOMAIN,
    CHAIN,
    GENESIS,
    encodePassportAttestationBody({
      version: 1,
      issuer: addr(1),
      subject: addr(2),
      category: "Code",
      evidenceHash: hash(3),
      issuerPolicyHash: hash(4),
      issuedEpoch: 5,
      expiresEpoch: 10,
      nonce: 7,
    }),
  ),
  "1d00000061676f72612d70617373706f72742d6174746573746174696f6e2d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef010000000101010101010101010101010101010101010101020202020202020202020202020202020202020200030303030303030303030303030303030303030303030303030303030303030304040404040404040404040404040404040404040404040404040404040404040500000000000000010a000000000000000700000000000000",
);
lock(
  "hub_registration",
  encodeBoundEnvelope(
    HUB_REGISTRATION_DOMAIN,
    CHAIN,
    GENESIS,
    encodeHubRegistrationBody({
      version: 1,
      publicName: "Agora Hub",
      classification: "Geographic",
      charterHash: hash(2),
      coordinators: [addr(1)],
      treasuryMultisig: addr(2),
      electionTermEpochs: 12,
      reportingIntervalEpochs: 3,
      coiDisclosureRoot: hash(3),
      deliverablesRoot: hash(4),
      accreditationProposalId: 1,
      nonce: 0,
    }),
  ),
  "1900000061676f72612d6875622d726567697374726174696f6e2d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef010000000900000041676f7261204875620a00000047656f67726170686963020202020202020202020202020202020202020202020202020202020202020201000000010101010101010101010101010101010101010102020202020202020202020202020202020202020c0000000000000003000000000000000303030303030303030303030303030303030303030303030303030303030303040404040404040404040404040404040404040404040404040404040404040401000000000000000000000000000000",
);
lock(
  "grant_registration",
  encodeBoundEnvelope(
    GRANT_REGISTRATION_DOMAIN,
    CHAIN,
    GENESIS,
    encodeGrantRegistrationBody({
      version: 1,
      registrar: addr(1),
      proposalId: 7,
      treasury: "OvlBuilder",
      beneficiary: addr(2),
      total: 10,
      kind: "Micro",
      milestones: [],
      coiDisclosureHash: hash(0),
      nonce: 0,
    }),
  ),
  "1b00000061676f72612d6772616e742d726567697374726174696f6e2d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef01000000010101010101010101010101010101010101010107000000000000000102020202020202020202020202020202020202020a00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
);
lock(
  "mission_registration",
  encodeBoundEnvelope(
    MISSION_REGISTRATION_DOMAIN,
    CHAIN,
    GENESIS,
    encodeMissionRegistrationBody({
      version: 1,
      sponsor: addr(1),
      rewardTreasury: "DrcCommunity",
      reward: 5,
      requirementsHash: hash(3),
      nonce: 0,
    }),
  ),
  "1d00000061676f72612d6d697373696f6e2d726567697374726174696f6e2d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef01000000010101010101010101010101010101010101010102050000000000000003030303030303030303030303030303030303030303030303030303030303030000000000000000",
);
lock(
  "treasury_disbursement",
  encodeBoundEnvelope(
    TREASURY_DISBURSEMENT_DOMAIN,
    CHAIN,
    GENESIS,
    encodeTreasuryDisbursementBody({
      version: 1,
      treasury: "OvlBuilder",
      beneficiary: addr(2),
      amount: 10,
      reasonHash: hash(3),
      authorizationRoot: hash(4),
      nonce: 0,
    }),
  ),
  "1e00000061676f72612d74726561737572792d64697362757273656d656e742d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef010000000102020202020202020202020202020202020202020a00000000000000030303030303030303030303030303030303030303030303030303030303030304040404040404040404040404040404040404040404040404040404040404040000000000000000",
);
lock(
  "vesting_unlock",
  encodeBoundEnvelope(
    VESTING_UNLOCK_DOMAIN,
    CHAIN,
    GENESIS,
    encodeVestingUnlockBody({
      version: 1,
      asset: "OVL",
      beneficiary: addr(2),
      scheduleId: hash(3),
      amount: 10,
      nonce: 0,
    }),
  ),
  "1700000061676f72612d76657374696e672d756e6c6f636b2d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0100000001020202020202020202020202020202020202020203030303030303030303030303030303030303030303030303030303030303030a000000000000000000000000000000",
);

assert.equal(DRC_WALLET_FAMILY_LANES.length, 18);
assert.equal(drcFamilyNeedsAmount("drc-channel-claim"), true);
assert.equal(drcFamilyNeedsObjectId("drc-channel-close"), true);
assert.equal(drcFamilyNeedsRecipient("drc-channel-create"), true);
assert.equal(drcFamilyNeedsAmount("drc-escrow-create"), true);
assert.equal(drcFamilyNeedsAmount("drc-escrow-finish"), false);
assert.equal(drcFamilyNeedsObjectId("drc-check-cash"), true);
assert.equal(drcFamilyNeedsIssued("drc-issued-transfer"), true);
assert.equal(drcFamilyNeedsRecipient("drc-ticket"), false);
assert.equal(drcFamilyNeedsExtraHex("drc-channel-claim"), true);
assert.equal(drcFamilyNeedsSignerList("drc-signer-list"), true);
assert.equal(drcFamilyNeedsPolicyAction("drc-account-policy"), true);
assert.equal(DRC_ACCOUNT_POLICY_ACTIONS.length, 6);
assert.deepEqual(parseDrcSignerEntries("agoradev1qqqq,2\nagoratest1qqqq 3"), [
  { signer: "agoradev1qqqq", weight: 2 },
  { signer: "agoratest1qqqq", weight: 3 },
]);
assert.throws(() => parseDrcSignerEntries(""), /at least one/);

console.log("typed-lane Borsh preimages match agora-types");
