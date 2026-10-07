/**
 * Device-local builders for remaining DRC families. Keys stay in the vault.
 * Encodings match agora-types `signing_bytes_bound` (Borsh, secp256k1).
 */

import { sha256 } from "@noble/hashes/sha256";
import { addressHrpForNetwork, encodeAddress } from "./address.ts";
import type { LightClient } from "./rpc.ts";
import {
  accountFromMnemonic,
  accountNonce,
  bytesToHex,
  concat,
  encodeBoundEnvelope,
  encodeOptionU32,
  encodeOptionU64,
  encodeVec,
  hexToBytes,
  jsonAddress,
  jsonBytes,
  nodeBinding,
  parseRecipient,
  requireAddress,
  requireHash,
  signBound,
  standardIssuedCurrency,
  u16,
  u32,
  u64,
  u8,
  type BuiltTypedEnvelope,
} from "./typed-lanes.ts";
import { chainIdForNetwork, signTransactionBody } from "./wallet.ts";

const NONE = u8(0);

export const DRC_ESCROW_CREATE_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-escrow-create-v1",
);
export const DRC_ESCROW_FINISH_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-escrow-finish-v1",
);
export const DRC_ESCROW_CANCEL_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-escrow-cancel-v1",
);
export const DRC_CHECK_CREATE_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-check-create-v1",
);
export const DRC_CHECK_CASH_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-check-cash-v1",
);
export const DRC_CHECK_CANCEL_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-check-cancel-v1",
);
export const DRC_CHANNEL_CREATE_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-payment-channel-create-v1",
);
export const DRC_CHANNEL_FUND_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-payment-channel-fund-v1",
);
export const DRC_CHANNEL_CLAIM_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-payment-channel-claim-v1",
);
export const DRC_CHANNEL_CLOSE_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-payment-channel-close-v1",
);
export const DRC_TRUST_LINE_SET_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-trust-line-set-v1",
);
export const DRC_ISSUED_TRANSFER_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-issued-transfer-v1",
);
export const DRC_ISSUED_POLICY_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-issued-asset-policy-set-v1",
);
export const DRC_ISSUER_CONTROL_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-trust-line-issuer-control-v1",
);
export const DRC_CLAWBACK_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-issued-clawback-v1",
);
export const DRC_TICKET_CREATE_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-ticket-create-v1",
);
export const DRC_REGULAR_KEY_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-regular-key-v1",
);
export const DRC_SIGNER_LIST_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-signer-list-v1",
);
export const DRC_DEPOSIT_PREAUTH_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-deposit-preauth-v1",
);
export const DRC_ACCOUNT_POLICY_V1_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-account-policy-v1",
);
export const DRC_ACCOUNT_POLICY_V2_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-account-policy-v2",
);
export const DRC_ACCOUNT_POLICY_V3_SIGNING_DOMAIN = new TextEncoder().encode(
  "agora-trident-drc-account-policy-v3",
);

const T_ESCROW_CREATE = new TextEncoder().encode("drc_escrow_create");
const T_ESCROW_FINISH = new TextEncoder().encode("drc_escrow_finish");
const T_ESCROW_CANCEL = new TextEncoder().encode("drc_escrow_cancel");
const T_CHECK_CREATE = new TextEncoder().encode("drc_check_create");
const T_CHECK_CASH = new TextEncoder().encode("drc_check_cash");
const T_CHECK_CANCEL = new TextEncoder().encode("drc_check_cancel");
const T_CHANNEL_CREATE = new TextEncoder().encode("drc_payment_channel_create");
const T_CHANNEL_FUND = new TextEncoder().encode("drc_payment_channel_fund");
const T_CHANNEL_CLAIM = new TextEncoder().encode("drc_payment_channel_claim");
const T_CHANNEL_CLOSE = new TextEncoder().encode("drc_payment_channel_close");
const T_TICKET = new TextEncoder().encode("drc_ticket_create");
const T_REGULAR_KEY = new TextEncoder().encode("drc_regular_key");
const T_SIGNER_LIST = new TextEncoder().encode("drc_signer_list");
const T_PREAUTH = new TextEncoder().encode("drc_deposit_preauth");
const T_POLICY = new TextEncoder().encode("drc_account_policy");

function typed(txType: Uint8Array, fields: Uint8Array): Uint8Array {
  return concat([encodeVec(txType), fields]);
}

type AuthOpts = {
  mnemonic: string;
  accountIndex?: number;
  network?: string;
  genesisHash: string;
  chainId?: string;
  fee: number;
  nonce: number;
};

function bind(options: AuthOpts) {
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(options.mnemonic, network, options.accountIndex ?? 0);
  const from = hexToBytes(account.addressHex);
  const chainId = options.chainId ?? chainIdForNetwork(network);
  return { network, account, from, chainId };
}

function envelope(
  account: ReturnType<typeof accountFromMnemonic>,
  network: string,
  from: Uint8Array,
  toBytes: Uint8Array | null,
  amount: number,
  fee: number,
  signingBytes: Uint8Array,
  tx: Record<string, unknown>,
): BuiltTypedEnvelope {
  const to = toBytes ? bytesToHex(toBytes) : bytesToHex(from);
  return {
    tx,
    from: account.addressHex,
    fromBech32: account.addressBech32,
    to,
    toBech32: encodeAddress(to, addressHrpForNetwork(network)),
    amount,
    fee,
    signingBytes,
  };
}

async function resolveFeeNonce(
  client: LightClient,
  mnemonic: string,
  network: string,
  accountIndex: number,
  fee?: number,
): Promise<{ fee: number; nonce: number; bind: Awaited<ReturnType<typeof nodeBinding>> }> {
  const account = accountFromMnemonic(mnemonic, network, accountIndex);
  const bindInfo = await nodeBinding(client, network);
  let resolved = fee;
  if (resolved === undefined) {
    try {
      resolved = (await client.estimateFee()).suggested_fee;
    } catch {
      resolved = 1;
    }
  }
  const nonce = await accountNonce(client, account.addressHex, "DRC");
  return { fee: resolved, nonce, bind: bindInfo };
}

export function encodeDrcEscrowCreateBody(fields: {
  version: number;
  owner: Uint8Array;
  recipient: Uint8Array;
  amount: number;
  fee: number;
  destinationTag?: number | null;
  sourceTag?: number | null;
  invoiceId: Uint8Array;
  finishAfter?: number | null;
  cancelAfter?: number | null;
  nonce: number;
}): Uint8Array {
  return typed(
    T_ESCROW_CREATE,
    concat([
      u32(fields.version),
      requireAddress(fields.owner),
      requireAddress(fields.recipient),
      u64(fields.amount),
      u64(fields.fee),
      encodeOptionU32(fields.destinationTag),
      encodeOptionU32(fields.sourceTag),
      requireHash(fields.invoiceId),
      encodeOptionU64(fields.finishAfter),
      encodeOptionU64(fields.cancelAfter),
      u64(fields.nonce),
    ]),
  );
}

export function encodeDrcEscrowFinishBody(fields: {
  version: number;
  submitter: Uint8Array;
  escrowId: Uint8Array;
  fee: number;
  nonce: number;
}): Uint8Array {
  return typed(
    T_ESCROW_FINISH,
    concat([
      u32(fields.version),
      requireAddress(fields.submitter),
      requireHash(fields.escrowId),
      u64(fields.fee),
      u64(fields.nonce),
    ]),
  );
}

export function encodeDrcEscrowCancelBody(fields: {
  version: number;
  submitter: Uint8Array;
  escrowId: Uint8Array;
  fee: number;
  nonce: number;
}): Uint8Array {
  return typed(
    T_ESCROW_CANCEL,
    concat([
      u32(fields.version),
      requireAddress(fields.submitter),
      requireHash(fields.escrowId),
      u64(fields.fee),
      u64(fields.nonce),
    ]),
  );
}

export function encodeDrcCheckCreateBody(fields: {
  version: number;
  owner: Uint8Array;
  destination: Uint8Array;
  amount: number;
  fee: number;
  destinationTag?: number | null;
  sourceTag?: number | null;
  invoiceId: Uint8Array;
  expiresAfter?: number | null;
  nonce: number;
}): Uint8Array {
  return typed(
    T_CHECK_CREATE,
    concat([
      u32(fields.version),
      requireAddress(fields.owner),
      requireAddress(fields.destination),
      u64(fields.amount),
      u64(fields.fee),
      encodeOptionU32(fields.destinationTag),
      encodeOptionU32(fields.sourceTag),
      requireHash(fields.invoiceId),
      encodeOptionU64(fields.expiresAfter),
      u64(fields.nonce),
    ]),
  );
}

export function encodeDrcCheckCashBody(fields: {
  version: number;
  submitter: Uint8Array;
  checkId: Uint8Array;
  fee: number;
  nonce: number;
}): Uint8Array {
  return typed(
    T_CHECK_CASH,
    concat([
      u32(fields.version),
      requireAddress(fields.submitter),
      requireHash(fields.checkId),
      u64(fields.fee),
      u64(fields.nonce),
    ]),
  );
}

export function encodeDrcCheckCancelBody(fields: {
  version: number;
  submitter: Uint8Array;
  checkId: Uint8Array;
  fee: number;
  nonce: number;
}): Uint8Array {
  return typed(
    T_CHECK_CANCEL,
    concat([
      u32(fields.version),
      requireAddress(fields.submitter),
      requireHash(fields.checkId),
      u64(fields.fee),
      u64(fields.nonce),
    ]),
  );
}

export function encodeDrcChannelCreateBody(fields: {
  version: number;
  owner: Uint8Array;
  destination: Uint8Array;
  amount: number;
  fee: number;
  claimPublicKey: Uint8Array;
  settleDelay: number;
  destinationTag?: number | null;
  sourceTag?: number | null;
  invoiceId: Uint8Array;
  cancelAfter?: number | null;
  nonce: number;
}): Uint8Array {
  return typed(
    T_CHANNEL_CREATE,
    concat([
      u32(fields.version),
      requireAddress(fields.owner),
      requireAddress(fields.destination),
      u64(fields.amount),
      u64(fields.fee),
      encodeVec(fields.claimPublicKey),
      u64(fields.settleDelay),
      encodeOptionU32(fields.destinationTag),
      encodeOptionU32(fields.sourceTag),
      requireHash(fields.invoiceId),
      encodeOptionU64(fields.cancelAfter),
      u64(fields.nonce),
    ]),
  );
}

export function encodeDrcChannelFundBody(fields: {
  version: number;
  submitter: Uint8Array;
  channelId: Uint8Array;
  amount: number;
  fee: number;
  nonce: number;
}): Uint8Array {
  return typed(
    T_CHANNEL_FUND,
    concat([
      u32(fields.version),
      requireAddress(fields.submitter),
      requireHash(fields.channelId),
      u64(fields.amount),
      u64(fields.fee),
      u64(fields.nonce),
    ]),
  );
}

export function encodeDrcChannelClaimBody(fields: {
  version: number;
  submitter: Uint8Array;
  channelId: Uint8Array;
  cumulativeAuthorized: number;
  claimSignature: Uint8Array;
  fee: number;
  nonce: number;
}): Uint8Array {
  return typed(
    T_CHANNEL_CLAIM,
    concat([
      u32(fields.version),
      requireAddress(fields.submitter),
      requireHash(fields.channelId),
      u64(fields.cumulativeAuthorized),
      encodeVec(fields.claimSignature),
      u64(fields.fee),
      u64(fields.nonce),
    ]),
  );
}

export function encodeDrcChannelCloseBody(fields: {
  version: number;
  submitter: Uint8Array;
  channelId: Uint8Array;
  closeKind: number;
  fee: number;
  nonce: number;
}): Uint8Array {
  return typed(
    T_CHANNEL_CLOSE,
    concat([
      u32(fields.version),
      requireAddress(fields.submitter),
      requireHash(fields.channelId),
      u8(fields.closeKind),
      u64(fields.fee),
      u64(fields.nonce),
    ]),
  );
}

export function encodeDrcTicketCreateBody(fields: {
  version: number;
  owner: Uint8Array;
  nonce: number;
  fee: number;
}): Uint8Array {
  return typed(
    T_TICKET,
    concat([
      u32(fields.version),
      requireAddress(fields.owner),
      u64(fields.nonce),
      u64(fields.fee),
    ]),
  );
}

export function encodeDrcRegularKeyBody(fields: {
  version: number;
  owner: Uint8Array;
  action: number;
  regularKey: Uint8Array;
  regularKeyPublicKey: Uint8Array;
  nonce: number;
  fee: number;
}): Uint8Array {
  return typed(
    T_REGULAR_KEY,
    concat([
      u32(fields.version),
      requireAddress(fields.owner),
      u8(fields.action),
      requireAddress(fields.regularKey),
      encodeVec(fields.regularKeyPublicKey),
      u64(fields.nonce),
      u64(fields.fee),
    ]),
  );
}

export function encodeDrcSignerListBody(fields: {
  version: number;
  owner: Uint8Array;
  action: number;
  quorum: number;
  entries: { signer: Uint8Array; weight: number }[];
  nonce: number;
  fee: number;
}): Uint8Array {
  const entryParts = [u32(fields.entries.length)];
  for (const entry of fields.entries) {
    entryParts.push(requireAddress(entry.signer), u16(entry.weight));
  }
  return typed(
    T_SIGNER_LIST,
    concat([
      u32(fields.version),
      requireAddress(fields.owner),
      u8(fields.action),
      u32(fields.quorum),
      concat(entryParts),
      u64(fields.nonce),
      u64(fields.fee),
    ]),
  );
}

export function encodeDrcDepositPreauthBody(fields: {
  version: number;
  owner: Uint8Array;
  action: number;
  authorizedSource: Uint8Array;
  nonce: number;
  fee: number;
}): Uint8Array {
  return typed(
    T_PREAUTH,
    concat([
      u32(fields.version),
      requireAddress(fields.owner),
      u8(fields.action),
      requireAddress(fields.authorizedSource),
      u64(fields.nonce),
      u64(fields.fee),
    ]),
  );
}

export function encodeDrcAccountPolicyV1Body(fields: {
  version: number;
  account: Uint8Array;
  action: number;
  fee: number;
  nonce: number;
}): Uint8Array {
  return concat([
    u32(fields.version),
    requireAddress(fields.account),
    u8(fields.action),
    u64(fields.fee),
    u64(fields.nonce),
  ]);
}

export function encodeDrcAccountPolicyV2Body(fields: {
  version: number;
  account: Uint8Array;
  action: number;
  fee: number;
  nonce: number;
}): Uint8Array {
  return typed(
    T_POLICY,
    concat([
      u32(fields.version),
      requireAddress(fields.account),
      u8(fields.action),
      u64(fields.fee),
      u64(fields.nonce),
    ]),
  );
}

export function encodeDrcTrustLineSetBody(fields: {
  version: number;
  holder: Uint8Array;
  issuer: Uint8Array;
  currency: Uint8Array;
  limit: number;
  fee: number;
  nonce: number;
}): Uint8Array {
  return concat([
    u32(fields.version),
    requireAddress(fields.holder),
    requireAddress(fields.issuer),
    requireAddress(fields.currency),
    u64(fields.limit),
    u64(fields.fee),
    u64(fields.nonce),
    NONE,
  ]);
}

export function encodeDrcIssuedTransferBody(fields: {
  version: number;
  sender: Uint8Array;
  recipient: Uint8Array;
  issuer: Uint8Array;
  currency: Uint8Array;
  amount: number;
  fee: number;
  destinationTag?: number | null;
  sourceTag?: number | null;
  invoiceId: Uint8Array;
  nonce: number;
}): Uint8Array {
  return concat([
    u32(fields.version),
    requireAddress(fields.sender),
    requireAddress(fields.recipient),
    requireAddress(fields.issuer),
    requireAddress(fields.currency),
    u64(fields.amount),
    u64(fields.fee),
    encodeOptionU32(fields.destinationTag),
    encodeOptionU32(fields.sourceTag),
    requireHash(fields.invoiceId),
    u64(fields.nonce),
    NONE,
  ]);
}

export function encodeDrcIssuedPolicyBody(fields: {
  version: number;
  issuer: Uint8Array;
  currency: Uint8Array;
  action: number;
  fee: number;
  nonce: number;
}): Uint8Array {
  return concat([
    u32(fields.version),
    requireAddress(fields.issuer),
    requireAddress(fields.currency),
    u8(fields.action),
    u64(fields.fee),
    u64(fields.nonce),
    NONE,
  ]);
}

function hashBound(
  domain: Uint8Array,
  chainId: string,
  genesisHex: string,
  body: Uint8Array,
): Uint8Array {
  return sha256(encodeBoundEnvelope(domain, chainId, genesisHex, body));
}

function authFields(publicKey: Uint8Array, signature: Uint8Array) {
  return {
    public_key: jsonBytes(publicKey),
    signature: jsonBytes(signature),
  };
}

export async function buildSignedDrcEscrowCreate(
  options: AuthOpts & { recipient: string; amount: number },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const to = parseRecipient(options.recipient, network);
  const invoiceId = new Uint8Array(32);
  const body = encodeDrcEscrowCreateBody({
    version: 1,
    owner: from,
    recipient: to.bytes,
    amount: options.amount,
    fee: options.fee,
    invoiceId,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    DRC_ESCROW_CREATE_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, to.bytes, options.amount, options.fee, signed.signingBytes, {
    version: 1,
    owner: jsonAddress(from),
    recipient: jsonAddress(to.bytes),
    amount: options.amount,
    fee: options.fee,
    destination_tag: null,
    source_tag: null,
    invoice_id: jsonBytes(invoiceId),
    finish_after_blue_score: null,
    cancel_after_blue_score: null,
    nonce: options.nonce,
    ...authFields(signed.publicKey, signed.signature),
  });
}

export async function buildSignedDrcEscrowFinish(
  options: AuthOpts & { escrowId: string },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const escrowId = hexToBytes(options.escrowId);
  const body = encodeDrcEscrowFinishBody({
    version: 1,
    submitter: from,
    escrowId,
    fee: options.fee,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    DRC_ESCROW_FINISH_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, null, 0, options.fee, signed.signingBytes, {
    version: 1,
    submitter: jsonAddress(from),
    escrow_id: jsonBytes(requireHash(escrowId)),
    fee: options.fee,
    nonce: options.nonce,
    ...authFields(signed.publicKey, signed.signature),
  });
}

export async function buildSignedDrcEscrowCancel(
  options: AuthOpts & { escrowId: string },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const escrowId = hexToBytes(options.escrowId);
  const body = encodeDrcEscrowCancelBody({
    version: 1,
    submitter: from,
    escrowId,
    fee: options.fee,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    DRC_ESCROW_CANCEL_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, null, 0, options.fee, signed.signingBytes, {
    version: 1,
    submitter: jsonAddress(from),
    escrow_id: jsonBytes(requireHash(escrowId)),
    fee: options.fee,
    nonce: options.nonce,
    ...authFields(signed.publicKey, signed.signature),
  });
}

export async function buildSignedDrcCheckCreate(
  options: AuthOpts & { destination: string; amount: number },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const to = parseRecipient(options.destination, network);
  const invoiceId = new Uint8Array(32);
  const body = encodeDrcCheckCreateBody({
    version: 1,
    owner: from,
    destination: to.bytes,
    amount: options.amount,
    fee: options.fee,
    invoiceId,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    DRC_CHECK_CREATE_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, to.bytes, options.amount, options.fee, signed.signingBytes, {
    version: 1,
    owner: jsonAddress(from),
    destination: jsonAddress(to.bytes),
    amount: options.amount,
    fee: options.fee,
    destination_tag: null,
    source_tag: null,
    invoice_id: jsonBytes(invoiceId),
    expires_after_blue_score: null,
    nonce: options.nonce,
    ...authFields(signed.publicKey, signed.signature),
  });
}

export async function buildSignedDrcCheckCash(
  options: AuthOpts & { checkId: string },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const checkId = hexToBytes(options.checkId);
  const body = encodeDrcCheckCashBody({
    version: 1,
    submitter: from,
    checkId,
    fee: options.fee,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    DRC_CHECK_CASH_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, null, 0, options.fee, signed.signingBytes, {
    version: 1,
    submitter: jsonAddress(from),
    check_id: jsonBytes(requireHash(checkId)),
    fee: options.fee,
    nonce: options.nonce,
    ...authFields(signed.publicKey, signed.signature),
  });
}

export async function buildSignedDrcCheckCancel(
  options: AuthOpts & { checkId: string },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const checkId = hexToBytes(options.checkId);
  const body = encodeDrcCheckCancelBody({
    version: 1,
    submitter: from,
    checkId,
    fee: options.fee,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    DRC_CHECK_CANCEL_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, null, 0, options.fee, signed.signingBytes, {
    version: 1,
    submitter: jsonAddress(from),
    check_id: jsonBytes(requireHash(checkId)),
    fee: options.fee,
    nonce: options.nonce,
    ...authFields(signed.publicKey, signed.signature),
  });
}

export async function buildSignedDrcChannelCreate(
  options: AuthOpts & {
    destination: string;
    amount: number;
    claimPublicKey: Uint8Array;
    settleDelay: number;
  },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const to = parseRecipient(options.destination, network);
  const invoiceId = new Uint8Array(32);
  const body = encodeDrcChannelCreateBody({
    version: 1,
    owner: from,
    destination: to.bytes,
    amount: options.amount,
    fee: options.fee,
    claimPublicKey: options.claimPublicKey,
    settleDelay: options.settleDelay,
    invoiceId,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    DRC_CHANNEL_CREATE_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, to.bytes, options.amount, options.fee, signed.signingBytes, {
    version: 1,
    owner: jsonAddress(from),
    destination: jsonAddress(to.bytes),
    amount: options.amount,
    fee: options.fee,
    claim_public_key: jsonBytes(options.claimPublicKey),
    settle_delay_blue_scores: options.settleDelay,
    destination_tag: null,
    source_tag: null,
    invoice_id: jsonBytes(invoiceId),
    cancel_after_blue_score: null,
    nonce: options.nonce,
    ...authFields(signed.publicKey, signed.signature),
  });
}

export async function buildSignedDrcChannelFund(
  options: AuthOpts & { channelId: string; amount: number },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const channelId = hexToBytes(options.channelId);
  const body = encodeDrcChannelFundBody({
    version: 1,
    submitter: from,
    channelId,
    amount: options.amount,
    fee: options.fee,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    DRC_CHANNEL_FUND_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, null, options.amount, options.fee, signed.signingBytes, {
    version: 1,
    submitter: jsonAddress(from),
    channel_id: jsonBytes(requireHash(channelId)),
    amount: options.amount,
    fee: options.fee,
    nonce: options.nonce,
    ...authFields(signed.publicKey, signed.signature),
  });
}

export async function buildSignedDrcChannelClaim(
  options: AuthOpts & {
    channelId: string;
    cumulativeAuthorized: number;
    claimSignature: Uint8Array;
  },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const channelId = hexToBytes(options.channelId);
  const body = encodeDrcChannelClaimBody({
    version: 1,
    submitter: from,
    channelId,
    cumulativeAuthorized: options.cumulativeAuthorized,
    claimSignature: options.claimSignature,
    fee: options.fee,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    DRC_CHANNEL_CLAIM_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, null, options.cumulativeAuthorized, options.fee, signed.signingBytes, {
    version: 1,
    submitter: jsonAddress(from),
    channel_id: jsonBytes(requireHash(channelId)),
    cumulative_authorized: options.cumulativeAuthorized,
    channel_claim_signature: jsonBytes(options.claimSignature),
    fee: options.fee,
    nonce: options.nonce,
    ...authFields(signed.publicKey, signed.signature),
  });
}

export async function buildSignedDrcChannelClose(
  options: AuthOpts & { channelId: string; closeKind?: number },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const channelId = hexToBytes(options.channelId);
  const closeKind = options.closeKind ?? 1;
  const body = encodeDrcChannelCloseBody({
    version: 1,
    submitter: from,
    channelId,
    closeKind,
    fee: options.fee,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    DRC_CHANNEL_CLOSE_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, null, 0, options.fee, signed.signingBytes, {
    version: 1,
    submitter: jsonAddress(from),
    channel_id: jsonBytes(requireHash(channelId)),
    close_kind:
      closeKind === 1
        ? "OwnerScheduleClose"
        : closeKind === 2
          ? "DestinationClose"
          : "Finalize",
    fee: options.fee,
    nonce: options.nonce,
    ...authFields(signed.publicKey, signed.signature),
  });
}

export async function buildSignedDrcTicketCreate(
  options: AuthOpts,
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const body = encodeDrcTicketCreateBody({
    version: 1,
    owner: from,
    nonce: options.nonce,
    fee: options.fee,
  });
  const signed = await signBound(
    account.secretKey,
    DRC_TICKET_CREATE_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, null, 0, options.fee, signed.signingBytes, {
    version: 1,
    owner: jsonAddress(from),
    fee: options.fee,
    nonce: options.nonce,
    ...authFields(signed.publicKey, signed.signature),
  });
}

export async function buildSignedDrcRegularKey(
  options: AuthOpts & {
    action?: "set" | "clear";
    regularKey?: string;
    regularKeyPublicKey?: Uint8Array;
  },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const clear = (options.action ?? "clear") === "clear";
  const regularKey = clear
    ? new Uint8Array(20)
    : parseRecipient(options.regularKey ?? "", network).bytes;
  const pk = clear ? new Uint8Array() : (options.regularKeyPublicKey ?? new Uint8Array());
  const body = encodeDrcRegularKeyBody({
    version: 1,
    owner: from,
    action: clear ? 1 : 0,
    regularKey,
    regularKeyPublicKey: pk,
    nonce: options.nonce,
    fee: options.fee,
  });
  const signed = await signBound(
    account.secretKey,
    DRC_REGULAR_KEY_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, regularKey, 0, options.fee, signed.signingBytes, {
    version: 1,
    owner: jsonAddress(from),
    action: clear ? "clear" : "set",
    regular_key: jsonAddress(regularKey),
    regular_key_public_key: jsonBytes(pk),
    fee: options.fee,
    nonce: options.nonce,
    ...authFields(signed.publicKey, signed.signature),
  });
}

export async function buildSignedDrcSignerList(
  options: AuthOpts & {
    action?: "set" | "delete";
    quorum?: number;
    entries?: { signer: string; weight: number }[];
  },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const del = (options.action ?? "set") === "delete";
  const entries = del
    ? []
    : (options.entries ?? []).map((entry) => ({
        signer: parseRecipient(entry.signer, network).bytes,
        weight: entry.weight,
      }));
  const quorum = del ? 0 : (options.quorum ?? 1);
  const body = encodeDrcSignerListBody({
    version: 1,
    owner: from,
    action: del ? 1 : 0,
    quorum,
    entries,
    nonce: options.nonce,
    fee: options.fee,
  });
  const signed = await signBound(
    account.secretKey,
    DRC_SIGNER_LIST_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, null, 0, options.fee, signed.signingBytes, {
    version: 1,
    owner: jsonAddress(from),
    action: del ? "delete" : "set",
    quorum,
    entries: entries.map((entry) => ({
      signer: jsonAddress(entry.signer),
      weight: entry.weight,
    })),
    fee: options.fee,
    nonce: options.nonce,
    ...authFields(signed.publicKey, signed.signature),
  });
}

export async function buildSignedDrcDepositPreauth(
  options: AuthOpts & { authorizedSource: string; action?: "authorize" | "unauthorize" },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const source = parseRecipient(options.authorizedSource, network);
  const unauth = options.action === "unauthorize";
  const body = encodeDrcDepositPreauthBody({
    version: 1,
    owner: from,
    action: unauth ? 1 : 0,
    authorizedSource: source.bytes,
    nonce: options.nonce,
    fee: options.fee,
  });
  const signed = await signBound(
    account.secretKey,
    DRC_DEPOSIT_PREAUTH_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, source.bytes, 0, options.fee, signed.signingBytes, {
    version: 1,
    owner: jsonAddress(from),
    action: unauth ? "unauthorize" : "authorize",
    authorized_source: jsonAddress(source.bytes),
    fee: options.fee,
    nonce: options.nonce,
    ...authFields(signed.publicKey, signed.signature),
  });
}

export async function buildSignedDrcAccountPolicy(
  options: AuthOpts & {
    action:
      | "set_require_destination_tag"
      | "clear_require_destination_tag"
      | "set_deposit_auth_required"
      | "clear_deposit_auth_required"
      | "set_master_key_disabled"
      | "clear_master_key_disabled";
  },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const table: Record<string, { version: number; action: number; domain: Uint8Array }> = {
    set_require_destination_tag: {
      version: 1,
      action: 0,
      domain: DRC_ACCOUNT_POLICY_V1_SIGNING_DOMAIN,
    },
    clear_require_destination_tag: {
      version: 1,
      action: 1,
      domain: DRC_ACCOUNT_POLICY_V1_SIGNING_DOMAIN,
    },
    set_deposit_auth_required: {
      version: 2,
      action: 2,
      domain: DRC_ACCOUNT_POLICY_V2_SIGNING_DOMAIN,
    },
    clear_deposit_auth_required: {
      version: 2,
      action: 3,
      domain: DRC_ACCOUNT_POLICY_V2_SIGNING_DOMAIN,
    },
    set_master_key_disabled: {
      version: 3,
      action: 4,
      domain: DRC_ACCOUNT_POLICY_V3_SIGNING_DOMAIN,
    },
    clear_master_key_disabled: {
      version: 3,
      action: 5,
      domain: DRC_ACCOUNT_POLICY_V3_SIGNING_DOMAIN,
    },
  };
  const spec = table[options.action];
  const body =
    spec.version === 1
      ? encodeDrcAccountPolicyV1Body({
          version: spec.version,
          account: from,
          action: spec.action,
          fee: options.fee,
          nonce: options.nonce,
        })
      : encodeDrcAccountPolicyV2Body({
          version: spec.version,
          account: from,
          action: spec.action,
          fee: options.fee,
          nonce: options.nonce,
        });
  const signed = await signBound(
    account.secretKey,
    spec.domain,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, null, 0, options.fee, signed.signingBytes, {
    version: spec.version,
    account: jsonAddress(from),
    action: options.action,
    fee: options.fee,
    nonce: options.nonce,
    ...authFields(signed.publicKey, signed.signature),
  });
}

export async function buildSignedDrcTrustLineSet(
  options: AuthOpts & { issuer: string; currency: string; limit: number },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const issuer = parseRecipient(options.issuer, network);
  const currency = standardIssuedCurrency(options.currency);
  const body = encodeDrcTrustLineSetBody({
    version: 1,
    holder: from,
    issuer: issuer.bytes,
    currency,
    limit: options.limit,
    fee: options.fee,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    DRC_TRUST_LINE_SET_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, issuer.bytes, options.limit, options.fee, signed.signingBytes, {
    version: 1,
    holder: jsonAddress(from),
    issuer: jsonAddress(issuer.bytes),
    currency: jsonBytes(currency),
    limit: options.limit,
    fee: options.fee,
    nonce: options.nonce,
    ...authFields(signed.publicKey, signed.signature),
  });
}

export async function buildSignedDrcIssuedTransfer(
  options: AuthOpts & {
    recipient: string;
    issuer: string;
    currency: string;
    amount: number;
  },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const recipient = parseRecipient(options.recipient, network);
  const issuer = parseRecipient(options.issuer, network);
  const currency = standardIssuedCurrency(options.currency);
  const invoiceId = new Uint8Array(32);
  const body = encodeDrcIssuedTransferBody({
    version: 1,
    sender: from,
    recipient: recipient.bytes,
    issuer: issuer.bytes,
    currency,
    amount: options.amount,
    fee: options.fee,
    invoiceId,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    DRC_ISSUED_TRANSFER_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, recipient.bytes, options.amount, options.fee, signed.signingBytes, {
    version: 1,
    sender: jsonAddress(from),
    recipient: jsonAddress(recipient.bytes),
    issuer: jsonAddress(issuer.bytes),
    currency: jsonBytes(currency),
    amount: options.amount,
    fee: options.fee,
    destination_tag: null,
    source_tag: null,
    invoice_id: jsonBytes(invoiceId),
    nonce: options.nonce,
    ...authFields(signed.publicKey, signed.signature),
  });
}

async function signHashed(
  secretKey: Uint8Array,
  domain: Uint8Array,
  chainId: string,
  genesisHex: string,
  body: Uint8Array,
) {
  const signingBytes = hashBound(domain, chainId, genesisHex, body);
  const { publicKey, signature } = await signTransactionBody(secretKey, signingBytes);
  return { publicKey, signature, signingBytes };
}

export async function buildSignedDrcIssuedAssetPolicySet(
  options: AuthOpts & { currency: string; action?: number },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const currency = standardIssuedCurrency(options.currency);
  const action = options.action ?? 1;
  const actionName = [
    "EnableRequireAuth",
    "EnableGlobalFreeze",
    "ClearGlobalFreeze",
    "EnableNoFreeze",
    "EnableClawback",
  ][action];
  if (!actionName) throw new Error("unknown issued-asset policy action");
  const body = encodeDrcIssuedPolicyBody({
    version: 1,
    issuer: from,
    currency,
    action,
    fee: options.fee,
    nonce: options.nonce,
  });
  const hashed = await signHashed(
    account.secretKey,
    DRC_ISSUED_POLICY_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, null, 0, options.fee, hashed.signingBytes, {
    version: 1,
    issuer: jsonAddress(from),
    currency: jsonBytes(currency),
    action: actionName,
    fee: options.fee,
    nonce: options.nonce,
    ...authFields(hashed.publicKey, hashed.signature),
  });
}

export async function buildSignedDrcTrustLineIssuerControl(
  options: AuthOpts & { holder: string; currency: string },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const holder = parseRecipient(options.holder, network);
  const currency = standardIssuedCurrency(options.currency);
  const body = concat([
    u32(1),
    requireAddress(from),
    requireAddress(holder.bytes),
    requireAddress(currency),
    u8(0),
    u64(options.fee),
    u64(options.nonce),
    NONE,
  ]);
  const hashed = await signHashed(
    account.secretKey,
    DRC_ISSUER_CONTROL_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, holder.bytes, 0, options.fee, hashed.signingBytes, {
    version: 1,
    issuer: jsonAddress(from),
    holder: jsonAddress(holder.bytes),
    currency: jsonBytes(currency),
    action: "AuthorizeHolder",
    fee: options.fee,
    nonce: options.nonce,
    ...authFields(hashed.publicKey, hashed.signature),
  });
}

export async function buildSignedDrcIssuedClawback(
  options: AuthOpts & { holder: string; currency: string; amount: number },
): Promise<BuiltTypedEnvelope> {
  const { network, account, from, chainId } = bind(options);
  const holder = parseRecipient(options.holder, network);
  const currency = standardIssuedCurrency(options.currency);
  const body = concat([
    u32(1),
    requireAddress(from),
    requireAddress(holder.bytes),
    requireAddress(currency),
    u64(options.amount),
    u64(options.fee),
    u64(options.nonce),
    NONE,
  ]);
  const hashed = await signHashed(
    account.secretKey,
    DRC_CLAWBACK_SIGNING_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  return envelope(account, network, from, holder.bytes, options.amount, options.fee, hashed.signingBytes, {
    version: 1,
    issuer: jsonAddress(from),
    holder: jsonAddress(holder.bytes),
    currency: jsonBytes(currency),
    amount: options.amount,
    fee: options.fee,
    nonce: options.nonce,
    ...authFields(hashed.publicKey, hashed.signature),
  });
}

async function sendFamily<T extends { id: string }>(
  client: LightClient,
  options: { mnemonic: string; network?: string; accountIndex?: number; fee?: number },
  build: (resolved: AuthOpts) => Promise<BuiltTypedEnvelope>,
  submit: (client: LightClient, tx: Record<string, unknown>) => Promise<T>,
): Promise<{ id: string; built: BuiltTypedEnvelope }> {
  const network = options.network ?? "mainnet";
  const { fee, nonce, bind: bindInfo } = await resolveFeeNonce(
    client,
    options.mnemonic,
    network,
    options.accountIndex ?? 0,
    options.fee,
  );
  const built = await build({
    ...options,
    fee,
    nonce,
    genesisHash: bindInfo.genesisHash,
    chainId: bindInfo.chainId,
    network: bindInfo.network,
  });
  const result = await submit(client, built.tx);
  return { id: result.id, built };
}

export async function sendDrcEscrowCreate(
  client: LightClient,
  options: {
    mnemonic: string;
    recipient: string;
    amount: number;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) =>
      buildSignedDrcEscrowCreate({ ...resolved, recipient: options.recipient, amount: options.amount }),
    async (c, tx) => ({ id: (await c.submitDrcEscrowCreate(tx)).escrow_id }),
  );
}

export async function sendDrcEscrowFinish(
  client: LightClient,
  options: {
    mnemonic: string;
    escrowId: string;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) => buildSignedDrcEscrowFinish({ ...resolved, escrowId: options.escrowId }),
    async (c, tx) => ({ id: (await c.submitDrcEscrowFinish(tx)).finish_tx_id }),
  );
}

export async function sendDrcEscrowCancel(
  client: LightClient,
  options: {
    mnemonic: string;
    escrowId: string;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) => buildSignedDrcEscrowCancel({ ...resolved, escrowId: options.escrowId }),
    async (c, tx) => ({ id: (await c.submitDrcEscrowCancel(tx)).cancel_tx_id }),
  );
}

export async function sendDrcCheckCreate(
  client: LightClient,
  options: {
    mnemonic: string;
    destination: string;
    amount: number;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) =>
      buildSignedDrcCheckCreate({
        ...resolved,
        destination: options.destination,
        amount: options.amount,
      }),
    async (c, tx) => ({ id: (await c.submitDrcCheckCreate(tx)).check_id }),
  );
}

export async function sendDrcCheckCash(
  client: LightClient,
  options: {
    mnemonic: string;
    checkId: string;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) => buildSignedDrcCheckCash({ ...resolved, checkId: options.checkId }),
    async (c, tx) => ({ id: (await c.submitDrcCheckCash(tx)).cash_tx_id }),
  );
}

export async function sendDrcCheckCancel(
  client: LightClient,
  options: {
    mnemonic: string;
    checkId: string;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) => buildSignedDrcCheckCancel({ ...resolved, checkId: options.checkId }),
    async (c, tx) => ({ id: (await c.submitDrcCheckCancel(tx)).cancel_tx_id }),
  );
}

export async function sendDrcChannelCreate(
  client: LightClient,
  options: {
    mnemonic: string;
    destination: string;
    amount: number;
    claimPublicKey: Uint8Array;
    settleDelay: number;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) =>
      buildSignedDrcChannelCreate({
        ...resolved,
        destination: options.destination,
        amount: options.amount,
        claimPublicKey: options.claimPublicKey,
        settleDelay: options.settleDelay,
      }),
    async (c, tx) => ({ id: (await c.submitDrcPaymentChannelCreate(tx)).channel_id }),
  );
}

export async function sendDrcChannelFund(
  client: LightClient,
  options: {
    mnemonic: string;
    channelId: string;
    amount: number;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) =>
      buildSignedDrcChannelFund({
        ...resolved,
        channelId: options.channelId,
        amount: options.amount,
      }),
    async (c, tx) => ({ id: (await c.submitDrcPaymentChannelFund(tx)).fund_tx_id }),
  );
}

export async function sendDrcChannelClaim(
  client: LightClient,
  options: {
    mnemonic: string;
    channelId: string;
    cumulativeAuthorized: number;
    claimSignature: Uint8Array;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) =>
      buildSignedDrcChannelClaim({
        ...resolved,
        channelId: options.channelId,
        cumulativeAuthorized: options.cumulativeAuthorized,
        claimSignature: options.claimSignature,
      }),
    async (c, tx) => ({ id: (await c.submitDrcPaymentChannelClaim(tx)).claim_tx_id }),
  );
}

export async function sendDrcChannelClose(
  client: LightClient,
  options: {
    mnemonic: string;
    channelId: string;
    closeKind?: number;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) =>
      buildSignedDrcChannelClose({
        ...resolved,
        channelId: options.channelId,
        closeKind: options.closeKind,
      }),
    async (c, tx) => ({ id: (await c.submitDrcPaymentChannelClose(tx)).close_tx_id }),
  );
}

export async function sendDrcTicketCreate(
  client: LightClient,
  options: { mnemonic: string; fee?: number; network?: string; accountIndex?: number },
) {
  return sendFamily(
    client,
    options,
    (resolved) => buildSignedDrcTicketCreate(resolved),
    async (c, tx) => ({ id: (await c.submitDrcTicketCreate(tx)).ticket_create_tx_id }),
  );
}

export async function sendDrcRegularKey(
  client: LightClient,
  options: {
    mnemonic: string;
    action?: "set" | "clear";
    regularKey?: string;
    regularKeyPublicKey?: Uint8Array;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) =>
      buildSignedDrcRegularKey({
        ...resolved,
        action: options.action,
        regularKey: options.regularKey,
        regularKeyPublicKey: options.regularKeyPublicKey,
      }),
    async (c, tx) => ({ id: (await c.submitDrcRegularKey(tx)).regular_key_tx_id }),
  );
}

export async function sendDrcSignerList(
  client: LightClient,
  options: {
    mnemonic: string;
    action?: "set" | "delete";
    quorum?: number;
    entries?: { signer: string; weight: number }[];
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) =>
      buildSignedDrcSignerList({
        ...resolved,
        action: options.action,
        quorum: options.quorum,
        entries: options.entries,
      }),
    async (c, tx) => ({ id: (await c.submitDrcSignerList(tx)).signer_list_tx_id }),
  );
}

export async function sendDrcDepositPreauth(
  client: LightClient,
  options: {
    mnemonic: string;
    authorizedSource: string;
    action?: "authorize" | "unauthorize";
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) =>
      buildSignedDrcDepositPreauth({
        ...resolved,
        authorizedSource: options.authorizedSource,
        action: options.action,
      }),
    async (c, tx) => ({ id: (await c.submitDrcDepositPreauth(tx)).preauth_tx_id }),
  );
}

export async function sendDrcAccountPolicy(
  client: LightClient,
  options: {
    mnemonic: string;
    action:
      | "set_require_destination_tag"
      | "clear_require_destination_tag"
      | "set_deposit_auth_required"
      | "clear_deposit_auth_required"
      | "set_master_key_disabled"
      | "clear_master_key_disabled";
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) => buildSignedDrcAccountPolicy({ ...resolved, action: options.action }),
    async (c, tx) => ({ id: (await c.submitDrcAccountPolicy(tx)).policy_tx_id }),
  );
}

export async function sendDrcTrustLineSet(
  client: LightClient,
  options: {
    mnemonic: string;
    issuer: string;
    currency: string;
    limit: number;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) =>
      buildSignedDrcTrustLineSet({
        ...resolved,
        issuer: options.issuer,
        currency: options.currency,
        limit: options.limit,
      }),
    async (c, tx) => ({ id: (await c.submitDrcTrustLineSet(tx)).trust_line_set_tx_id }),
  );
}

export async function sendDrcIssuedTransfer(
  client: LightClient,
  options: {
    mnemonic: string;
    recipient: string;
    issuer: string;
    currency: string;
    amount: number;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) =>
      buildSignedDrcIssuedTransfer({
        ...resolved,
        recipient: options.recipient,
        issuer: options.issuer,
        currency: options.currency,
        amount: options.amount,
      }),
    async (c, tx) => ({ id: (await c.submitDrcIssuedTransfer(tx)).issued_transfer_tx_id }),
  );
}

export async function sendDrcIssuedAssetPolicySet(
  client: LightClient,
  options: {
    mnemonic: string;
    currency: string;
    action?: number;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) =>
      buildSignedDrcIssuedAssetPolicySet({
        ...resolved,
        currency: options.currency,
        action: options.action,
      }),
    async (c, tx) => ({ id: (await c.submitDrcIssuedAssetPolicySet(tx)).policy_set_tx_id }),
  );
}

export async function sendDrcTrustLineIssuerControl(
  client: LightClient,
  options: {
    mnemonic: string;
    holder: string;
    currency: string;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) =>
      buildSignedDrcTrustLineIssuerControl({
        ...resolved,
        holder: options.holder,
        currency: options.currency,
      }),
    async (c, tx) => ({ id: (await c.submitDrcTrustLineIssuerControl(tx)).issuer_control_tx_id }),
  );
}

export async function sendDrcIssuedClawback(
  client: LightClient,
  options: {
    mnemonic: string;
    holder: string;
    currency: string;
    amount: number;
    fee?: number;
    network?: string;
    accountIndex?: number;
  },
) {
  return sendFamily(
    client,
    options,
    (resolved) =>
      buildSignedDrcIssuedClawback({
        ...resolved,
        holder: options.holder,
        currency: options.currency,
        amount: options.amount,
      }),
    async (c, tx) => ({ id: (await c.submitDrcIssuedClawback(tx)).clawback_tx_id }),
  );
}

/** Shared helper table so desktop/mobile can call every family without a dedicated screen. */
export const DRC_FAMILY_SENDERS = {
  sendDrcEscrowCreate,
  sendDrcEscrowFinish,
  sendDrcEscrowCancel,
  sendDrcCheckCreate,
  sendDrcCheckCash,
  sendDrcCheckCancel,
  sendDrcChannelCreate,
  sendDrcChannelFund,
  sendDrcChannelClaim,
  sendDrcChannelClose,
  sendDrcTicketCreate,
  sendDrcRegularKey,
  sendDrcSignerList,
  sendDrcDepositPreauth,
  sendDrcAccountPolicy,
  sendDrcTrustLineSet,
  sendDrcIssuedTransfer,
  sendDrcIssuedAssetPolicySet,
  sendDrcTrustLineIssuerControl,
  sendDrcIssuedClawback,
};
