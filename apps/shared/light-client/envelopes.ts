/**
 * Device-side builders for live Agora envelopes.
 *
 * Preimages follow `signing_bytes_bound` in `agora-types`. The node receives
 * an already signed JSON body. Issued-asset controls sign the SHA-256 of the
 * Borsh body, matching `Hash::hash_borsh` inside those methods.
 */

import { sha256 } from "@noble/hashes/sha256";
import * as secp from "@noble/secp256k1";

import { parseAddress } from "./address";
import {
  asSafeNumber,
  borshBytes,
  borshStr,
  bytesToHex,
  concat,
  hexToBytes,
  jsonBytes,
  optionBytes,
  repeatByte,
  u16,
  u32,
  u64,
  u8,
} from "./borshWire";
import { deriveAccount, type WalletAccount } from "./wallet";

export type BuiltEnvelope = {
  method: string;
  paramKey: string;
  body: Record<string, unknown>;
  preimageHex: string;
};

export type SequenceSelector = {
  kind: "nonce" | "ticket";
  value: bigint;
};

const RESERVED_CURRENCY = new Set(["DRC", "TLT", "OVL", "XRP"]);

export function addressBytes(input: string, network: string): Uint8Array {
  const hex = parseAddress(input, network);
  const bytes = hexToBytes(hex);
  if (bytes.length !== 20) throw new Error("address must be 20 bytes");
  return bytes;
}

export function issuedCurrency(code: string): Uint8Array {
  const raw = code.trim();
  const out = new Uint8Array(20);
  if (/^[A-Z0-9]{3}$/.test(raw)) {
    if (RESERVED_CURRENCY.has(raw)) {
      throw new Error(`${raw} is reserved and cannot be an issued currency code`);
    }
    out.set(new TextEncoder().encode(raw));
    return out;
  }
  const bytes = hexToBytes(raw);
  if (bytes.length !== 20 || bytes[0] === 0) {
    throw new Error("currency must be a 3-character code or 20 non-zero-prefix bytes");
  }
  return bytes;
}

export function hashBytes(hex: string | undefined, emptyIsZero = true): Uint8Array {
  const text = (hex ?? "").trim();
  if (!text) {
    if (emptyIsZero) return new Uint8Array(32);
    throw new Error("hash is required");
  }
  const bytes = hexToBytes(text);
  if (bytes.length !== 32) throw new Error("hash must be 32 bytes");
  return bytes;
}

function sequenceBytes(sequence: SequenceSelector): Uint8Array {
  return concat([u8(sequence.kind === "nonce" ? 0 : 1), u64(sequence.value)]);
}

function bound(
  domain: string,
  chainId: string,
  genesis: Uint8Array,
  rest: Uint8Array[],
): Uint8Array {
  if (genesis.length !== 32) throw new Error("genesis must be 32 bytes");
  return concat([borshBytes(new TextEncoder().encode(domain)), borshStr(chainId), genesis, ...rest]);
}

async function signPreimage(
  secretKey: Uint8Array,
  preimage: Uint8Array,
): Promise<{ publicKey: Uint8Array; signature: Uint8Array }> {
  const digest = sha256(preimage);
  const signature = await secp.signAsync(digest, secretKey, { lowS: true });
  const compact =
    signature instanceof Uint8Array
      ? signature.slice(0, 64)
      : (signature as { toCompactRawBytes: () => Uint8Array }).toCompactRawBytes();
  return { publicKey: secp.getPublicKey(secretKey, true), signature: compact };
}

function withSingleSig(
  body: Record<string, unknown>,
  publicKey: Uint8Array,
  signature: Uint8Array,
): Record<string, unknown> {
  return {
    ...body,
    public_key: jsonBytes(publicKey),
    signature: jsonBytes(signature),
  };
}

export async function signEnvelope(
  account: WalletAccount,
  preimage: Uint8Array,
  body: Record<string, unknown>,
  method: string,
  paramKey: string,
): Promise<BuiltEnvelope> {
  const signed = await signPreimage(account.secretKey, preimage);
  return {
    method,
    paramKey,
    body: withSingleSig(body, signed.publicKey, signed.signature),
    preimageHex: bytesToHex(preimage),
  };
}

export function requireSigner(mnemonic: string, network: string): WalletAccount {
  if (!mnemonic.trim()) throw new Error("Watch-only cannot spend");
  return deriveAccount(mnemonic, 0, "", network, 0);
}

function feeAmount(fee: bigint): number {
  if (fee <= 0n) throw new Error("DRC fee must be greater than zero");
  return asSafeNumber(fee, "fee");
}

function tagPair(destination: number | null, source: number | null): Uint8Array[] {
  return [
    optionBytes(destination === null ? null : u32(destination)),
    optionBytes(source === null ? null : u32(source)),
  ];
}

export function accountTransferPreimage(args: {
  asset: "OVL" | "DRC";
  from: Uint8Array;
  to: Uint8Array;
  amount: bigint;
  fee: bigint;
  nonce: bigint;
  chainId: string;
  genesis: Uint8Array;
}): Uint8Array {
  return bound("agora-trident-account-tx-v2", args.chainId, args.genesis, [
    u32(2),
    u8(args.asset === "OVL" ? 1 : 2),
    args.from,
    args.to,
    u64(args.amount),
    u64(args.fee),
    u64(args.nonce),
  ]);
}

export async function buildAccountTransfer(args: {
  account: WalletAccount;
  asset: "OVL" | "DRC";
  to: Uint8Array;
  amount: bigint;
  fee: bigint;
  nonce: bigint;
  chainId: string;
  genesis: Uint8Array;
}): Promise<BuiltEnvelope> {
  const from = hexToBytes(args.account.addressHex);
  const preimage = accountTransferPreimage({
    asset: args.asset,
    from,
    to: args.to,
    amount: args.amount,
    fee: args.fee,
    nonce: args.nonce,
    chainId: args.chainId,
    genesis: args.genesis,
  });
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 2,
      asset: args.asset,
      from: jsonBytes(from),
      to: jsonBytes(args.to),
      amount: asSafeNumber(args.amount, "amount"),
      fee: asSafeNumber(args.fee, "fee"),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitAccountTransfer",
    "account_transfer",
  );
}

export function paymentV4Preimage(args: {
  from: Uint8Array;
  to: Uint8Array;
  amount: bigint;
  fee: bigint;
  destinationTag: number | null;
  sourceTag: number | null;
  invoice: Uint8Array;
  nonce: bigint;
  expiry: bigint | null;
  chainId: string;
  genesis: Uint8Array;
}): Uint8Array {
  return bound("agora-trident-drc-payment-v4", args.chainId, args.genesis, [
    u32(4),
    args.from,
    args.to,
    u64(args.amount),
    u64(args.fee),
    ...tagPair(args.destinationTag, args.sourceTag),
    args.invoice,
    u64(args.nonce),
    optionBytes(args.expiry === null ? null : u64(args.expiry)),
  ]);
}

export async function buildDrcPayment(args: {
  account: WalletAccount;
  to: Uint8Array;
  amount: bigint;
  fee: bigint;
  destinationTag: number | null;
  sourceTag: number | null;
  invoice: Uint8Array;
  nonce: bigint;
  expiry: bigint | null;
  chainId: string;
  genesis: Uint8Array;
  multisignMnemonic?: string;
  network: string;
}): Promise<BuiltEnvelope> {
  const from = hexToBytes(args.account.addressHex);
  const preimage = paymentV4Preimage({
    from,
    to: args.to,
    amount: args.amount,
    fee: args.fee,
    destinationTag: args.destinationTag,
    sourceTag: args.sourceTag,
    invoice: args.invoice,
    nonce: args.nonce,
    expiry: args.expiry,
    chainId: args.chainId,
    genesis: args.genesis,
  });
  const body: Record<string, unknown> = {
    version: 4,
    from: jsonBytes(from),
    to: jsonBytes(args.to),
    amount: asSafeNumber(args.amount, "amount"),
    fee: feeAmount(args.fee),
    destination_tag: args.destinationTag,
    source_tag: args.sourceTag,
    invoice_id: jsonBytes(args.invoice),
    nonce: asSafeNumber(args.nonce, "nonce"),
    last_valid_blue_score:
      args.expiry === null ? null : asSafeNumber(args.expiry, "expiry"),
  };
  if (args.multisignMnemonic?.trim()) {
    const participant = deriveAccount(args.multisignMnemonic, 0, "", args.network, 0);
    const participantPreimage = bound(
      "agora-trident-drc-multisign-participant-v1",
      args.chainId,
      args.genesis,
      [u32(1), from, borshBytes(preimage)],
    );
    const signed = await signPreimage(participant.secretKey, participantPreimage);
    body.public_key = [];
    body.signature = [];
    body.multisign = {
      version: 1,
      signing_for: jsonBytes(from),
      signatures: [
        {
          signer: jsonBytes(hexToBytes(participant.addressHex)),
          public_key: jsonBytes(signed.publicKey),
          signature: jsonBytes(signed.signature),
        },
      ],
    };
    return {
      method: "agora_submitDrcPayment",
      paramKey: "payment",
      body,
      preimageHex: bytesToHex(preimage),
    };
  }
  return signEnvelope(args.account, preimage, body, "agora_submitDrcPayment", "payment");
}

const POLICY_ACTION: Record<string, { version: number; domain: string; discriminant: number; wire: string }> = {
  set_require_destination_tag: {
    version: 1,
    domain: "agora-trident-drc-account-policy-v1",
    discriminant: 0,
    wire: "set_require_destination_tag",
  },
  clear_require_destination_tag: {
    version: 1,
    domain: "agora-trident-drc-account-policy-v1",
    discriminant: 1,
    wire: "clear_require_destination_tag",
  },
  set_deposit_auth_required: {
    version: 2,
    domain: "agora-trident-drc-account-policy-v2",
    discriminant: 2,
    wire: "set_deposit_auth_required",
  },
  clear_deposit_auth_required: {
    version: 2,
    domain: "agora-trident-drc-account-policy-v2",
    discriminant: 3,
    wire: "clear_deposit_auth_required",
  },
  set_master_key_disabled: {
    version: 3,
    domain: "agora-trident-drc-account-policy-v3",
    discriminant: 4,
    wire: "set_master_key_disabled",
  },
  clear_master_key_disabled: {
    version: 3,
    domain: "agora-trident-drc-account-policy-v3",
    discriminant: 5,
    wire: "clear_master_key_disabled",
  },
};

export function policyPreimage(args: {
  action: string;
  account: Uint8Array;
  fee: bigint;
  nonce: bigint;
  chainId: string;
  genesis: Uint8Array;
}): Uint8Array {
  const spec = POLICY_ACTION[args.action];
  if (!spec) throw new Error("unknown account-policy action");
  const typed =
    spec.version === 1
      ? []
      : [borshBytes(new TextEncoder().encode("drc_account_policy"))];
  return bound(spec.domain, args.chainId, args.genesis, [
    ...typed,
    u32(spec.version),
    args.account,
    u8(spec.discriminant),
    u64(args.fee),
    u64(args.nonce),
  ]);
}

export async function buildAccountPolicy(args: {
  account: WalletAccount;
  action: string;
  fee: bigint;
  nonce: bigint;
  chainId: string;
  genesis: Uint8Array;
}): Promise<BuiltEnvelope> {
  const spec = POLICY_ACTION[args.action];
  if (!spec) throw new Error("unknown account-policy action");
  const owner = hexToBytes(args.account.addressHex);
  const preimage = policyPreimage({
    action: args.action,
    account: owner,
    fee: args.fee,
    nonce: args.nonce,
    chainId: args.chainId,
    genesis: args.genesis,
  });
  return signEnvelope(
    args.account,
    preimage,
    {
      version: spec.version,
      account: jsonBytes(owner),
      action: spec.wire,
      fee: feeAmount(args.fee),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcAccountPolicy",
    "policy",
  );
}

export function ovlExecutionPreimage(args: {
  from: Uint8Array;
  to: Uint8Array;
  value: bigint;
  gasLimit: bigint;
  maxFeePerGas: bigint;
  nonce: bigint;
  data: Uint8Array;
  chainId: string;
  genesis: Uint8Array;
}): Uint8Array {
  if (args.data.length !== 0) {
    throw new Error("OVL contract calls are not active");
  }
  if (bytesToHex(args.to) === "00".repeat(20)) {
    throw new Error("OVL contract creation is not active");
  }
  return bound("agora-trident-ovl-execution-v1", args.chainId, args.genesis, [
    u32(1),
    args.from,
    args.to,
    u64(args.value),
    u64(args.gasLimit),
    u64(args.maxFeePerGas),
    u64(args.nonce),
    borshBytes(args.data),
  ]);
}

export async function buildOvlValueCall(args: {
  account: WalletAccount;
  to: Uint8Array;
  value: bigint;
  maxFeePerGas: bigint;
  nonce: bigint;
  chainId: string;
  genesis: Uint8Array;
}): Promise<BuiltEnvelope> {
  const from = hexToBytes(args.account.addressHex);
  if (bytesToHex(from) === bytesToHex(args.to)) {
    throw new Error("OVL self-execution is forbidden");
  }
  if (args.maxFeePerGas <= 0n) throw new Error("max fee per gas must be positive");
  const gasLimit = 21_000n;
  const preimage = ovlExecutionPreimage({
    from,
    to: args.to,
    value: args.value,
    gasLimit,
    maxFeePerGas: args.maxFeePerGas,
    nonce: args.nonce,
    data: new Uint8Array(),
    chainId: args.chainId,
    genesis: args.genesis,
  });
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      from: jsonBytes(from),
      to: jsonBytes(args.to),
      value: asSafeNumber(args.value, "value"),
      gas_limit: 21000,
      max_fee_per_gas: asSafeNumber(args.maxFeePerGas, "max fee"),
      nonce: asSafeNumber(args.nonce, "nonce"),
      data: [],
    },
    "agora_submitOvlExecution",
    "execution",
  );
}

function typedPreimage(
  domain: string,
  txType: string,
  chainId: string,
  genesis: Uint8Array,
  rest: Uint8Array[],
): Uint8Array {
  return bound(domain, chainId, genesis, [
    borshBytes(new TextEncoder().encode(txType)),
    ...rest,
  ]);
}

export function escrowCreatePreimage(args: {
  owner: Uint8Array;
  recipient: Uint8Array;
  amount: bigint;
  fee: bigint;
  destinationTag: number | null;
  sourceTag: number | null;
  finishAfter: bigint | null;
  cancelAfter: bigint | null;
  nonce: bigint;
  chainId: string;
  genesis: Uint8Array;
}): Uint8Array {
  return typedPreimage(
    "agora-trident-drc-escrow-create-v1",
    "drc_escrow_create",
    args.chainId,
    args.genesis,
    [
      u32(1),
      args.owner,
      args.recipient,
      u64(args.amount),
      u64(args.fee),
      ...tagPair(args.destinationTag, args.sourceTag),
      new Uint8Array(32),
      optionBytes(args.finishAfter === null ? null : u64(args.finishAfter)),
      optionBytes(args.cancelAfter === null ? null : u64(args.cancelAfter)),
      u64(args.nonce),
    ],
  );
}

export function trustLinePreimage(args: {
  holder: Uint8Array;
  issuer: Uint8Array;
  currency: Uint8Array;
  limit: bigint;
  fee: bigint;
  nonce: bigint;
  chainId: string;
  genesis: Uint8Array;
}): Uint8Array {
  return bound("agora-trident-drc-trust-line-set-v1", args.chainId, args.genesis, [
    u32(1),
    args.holder,
    args.issuer,
    args.currency,
    u64(args.limit),
    u64(args.fee),
    u64(args.nonce),
    u8(0),
  ]);
}

export function issuedPolicyMessage(args: {
  issuer: Uint8Array;
  currency: Uint8Array;
  action: number;
  fee: bigint;
  nonce: bigint;
  chainId: string;
  genesis: Uint8Array;
}): Uint8Array {
  const body = bound(
    "agora-trident-drc-issued-asset-policy-set-v1",
    args.chainId,
    args.genesis,
    [u32(1), args.issuer, args.currency, u8(args.action), u64(args.fee), u64(args.nonce), u8(0)],
  );
  return sha256(body);
}

export function ticketCreatePreimage(args: {
  owner: Uint8Array;
  nonce: bigint;
  fee: bigint;
  chainId: string;
  genesis: Uint8Array;
}): Uint8Array {
  return typedPreimage(
    "agora-trident-drc-ticket-create-v1",
    "drc_ticket_create",
    args.chainId,
    args.genesis,
    [u32(1), args.owner, u64(args.nonce), u64(args.fee)],
  );
}

export function stakeBondPreimage(args: {
  asset: "OVL" | "DRC";
  actor: Uint8Array;
  amount: bigint;
  consensusPubkey: Uint8Array;
  withdrawal: Uint8Array;
  commissionBps: number;
  nonce: bigint;
  chainId: string;
  genesis: Uint8Array;
}): Uint8Array {
  return bound("agora-trident-stake-tx-v1", args.chainId, args.genesis, [
    u32(1),
    u8(args.asset === "OVL" ? 1 : 2),
    u8(0),
    args.actor,
    args.actor,
    u64(args.amount),
    borshBytes(args.consensusPubkey),
    args.withdrawal,
    u16(args.commissionBps),
    new Uint8Array(32),
    u64(args.nonce),
  ]);
}

export function fixturePreimages(): Record<string, string> {
  const chainId = "agora-dev";
  const genesis = repeatByte(7, 32);
  const from = repeatByte(1, 20);
  const to = repeatByte(2, 20);
  const issuer = repeatByte(3, 20);
  const currency = issuedCurrency("USD");
  return {
    account_ovl: bytesToHex(
      accountTransferPreimage({
        asset: "OVL",
        from,
        to,
        amount: 10n,
        fee: 1n,
        nonce: 3n,
        chainId,
        genesis,
      }),
    ),
    payment_v4: bytesToHex(
      paymentV4Preimage({
        from,
        to,
        amount: 3n,
        fee: 1n,
        destinationTag: 9,
        sourceTag: null,
        invoice: repeatByte(6, 32),
        nonce: 7n,
        expiry: 100n,
        chainId,
        genesis,
      }),
    ),
    policy_deposit_auth: bytesToHex(
      policyPreimage({
        action: "set_deposit_auth_required",
        account: from,
        fee: 1n,
        nonce: 4n,
        chainId,
        genesis,
      }),
    ),
    ovl_execution: bytesToHex(
      ovlExecutionPreimage({
        from,
        to,
        value: 5n,
        gasLimit: 21000n,
        maxFeePerGas: 2n,
        nonce: 0n,
        data: new Uint8Array(),
        chainId,
        genesis,
      }),
    ),
    escrow_create: bytesToHex(
      escrowCreatePreimage({
        owner: from,
        recipient: to,
        amount: 8n,
        fee: 1n,
        destinationTag: 4,
        sourceTag: null,
        finishAfter: 20n,
        cancelAfter: 40n,
        nonce: 5n,
        chainId,
        genesis,
      }),
    ),
    trust_line: bytesToHex(
      trustLinePreimage({
        holder: from,
        issuer,
        currency,
        limit: 50n,
        fee: 1n,
        nonce: 4n,
        chainId,
        genesis,
      }),
    ),
    issued_policy: bytesToHex(
      issuedPolicyMessage({
        issuer,
        currency,
        action: 0,
        fee: 1n,
        nonce: 4n,
        chainId,
        genesis,
      }),
    ),
    ticket_create: bytesToHex(
      ticketCreatePreimage({
        owner: from,
        nonce: 6n,
        fee: 1n,
        chainId,
        genesis,
      }),
    ),
    stake_bond: bytesToHex(
      stakeBondPreimage({
        asset: "OVL",
        actor: from,
        amount: 11n,
        consensusPubkey: repeatByte(0x02, 33),
        withdrawal: to,
        commissionBps: 100,
        nonce: 2n,
        chainId,
        genesis,
      }),
    ),
  };
}

type SimpleArgs = {
  account: WalletAccount;
  chainId: string;
  genesis: Uint8Array;
  fee: bigint;
  nonce: bigint;
};

function ownerOf(account: WalletAccount): Uint8Array {
  return hexToBytes(account.addressHex);
}

export async function buildDepositPreauth(
  args: SimpleArgs & { action: "authorize" | "unauthorize"; source: Uint8Array },
): Promise<BuiltEnvelope> {
  const owner = ownerOf(args.account);
  const discriminant = args.action === "authorize" ? 0 : 1;
  const preimage = typedPreimage(
    "agora-trident-drc-deposit-preauth-v1",
    "drc_deposit_preauth",
    args.chainId,
    args.genesis,
    [u32(1), owner, u8(discriminant), args.source, u64(args.nonce), u64(args.fee)],
  );
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      owner: jsonBytes(owner),
      action: args.action,
      authorized_source: jsonBytes(args.source),
      fee: feeAmount(args.fee),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcDepositPreauth",
    "preauth",
  );
}

export async function buildRegularKey(
  args: SimpleArgs & {
    action: "set" | "clear";
    regularKey: Uint8Array;
    regularKeyPublicKey: Uint8Array;
  },
): Promise<BuiltEnvelope> {
  const owner = ownerOf(args.account);
  const regular = args.action === "clear" ? new Uint8Array(20) : args.regularKey;
  const pubkey = args.action === "clear" ? new Uint8Array() : args.regularKeyPublicKey;
  if (args.action === "set" && pubkey.length !== 33) {
    throw new Error("regular key public key must be 33 bytes");
  }
  const preimage = typedPreimage(
    "agora-trident-drc-regular-key-v1",
    "drc_regular_key",
    args.chainId,
    args.genesis,
    [u32(1), owner, u8(args.action === "set" ? 0 : 1), regular, borshBytes(pubkey), u64(args.nonce), u64(args.fee)],
  );
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      owner: jsonBytes(owner),
      action: args.action,
      regular_key: jsonBytes(regular),
      regular_key_public_key: jsonBytes(pubkey),
      fee: feeAmount(args.fee),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcRegularKey",
    "regular_key",
  );
}

export async function buildSignerList(
  args: SimpleArgs & {
    action: "set" | "delete";
    quorum: number;
    signer: Uint8Array | null;
    weight: number;
  },
): Promise<BuiltEnvelope> {
  const owner = ownerOf(args.account);
  const entries =
    args.action === "delete" || !args.signer
      ? []
      : [{ signer: args.signer, weight: args.weight }];
  const quorum = args.action === "delete" ? 0 : args.quorum;
  const entryBytes = concat(
    entries.map((entry) => concat([entry.signer, u16(entry.weight)])),
  );
  const preimage = typedPreimage(
    "agora-trident-drc-signer-list-v1",
    "drc_signer_list",
    args.chainId,
    args.genesis,
    [
      u32(1),
      owner,
      u8(args.action === "set" ? 0 : 1),
      u32(quorum),
      borshBytes(entryBytes.length === 0 && entries.length === 0 ? new Uint8Array() : entryBytes).length
        ? concat([u32(entries.length), entryBytes])
        : u32(0),
      u64(args.nonce),
      u64(args.fee),
    ],
  );
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      owner: jsonBytes(owner),
      action: args.action,
      quorum,
      entries: entries.map((entry) => ({
        signer: jsonBytes(entry.signer),
        weight: entry.weight,
      })),
      fee: feeAmount(args.fee),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcSignerList",
    "signer_list",
  );
}

export async function buildTicketCreate(args: SimpleArgs): Promise<BuiltEnvelope> {
  const owner = ownerOf(args.account);
  const preimage = ticketCreatePreimage({
    owner,
    nonce: args.nonce,
    fee: args.fee,
    chainId: args.chainId,
    genesis: args.genesis,
  });
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      owner: jsonBytes(owner),
      fee: feeAmount(args.fee),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcTicketCreate",
    "ticket_create",
  );
}

export async function buildEscrowCreate(
  args: SimpleArgs & {
    recipient: Uint8Array;
    amount: bigint;
    destinationTag: number | null;
    sourceTag: number | null;
    finishAfter: bigint | null;
    cancelAfter: bigint | null;
  },
): Promise<BuiltEnvelope> {
  const owner = ownerOf(args.account);
  const preimage = escrowCreatePreimage({
    owner,
    recipient: args.recipient,
    amount: args.amount,
    fee: args.fee,
    destinationTag: args.destinationTag,
    sourceTag: args.sourceTag,
    finishAfter: args.finishAfter,
    cancelAfter: args.cancelAfter,
    nonce: args.nonce,
    chainId: args.chainId,
    genesis: args.genesis,
  });
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      owner: jsonBytes(owner),
      recipient: jsonBytes(args.recipient),
      amount: asSafeNumber(args.amount, "amount"),
      fee: feeAmount(args.fee),
      destination_tag: args.destinationTag,
      source_tag: args.sourceTag,
      invoice_id: jsonBytes(new Uint8Array(32)),
      finish_after_blue_score:
        args.finishAfter === null ? null : asSafeNumber(args.finishAfter, "finish after"),
      cancel_after_blue_score:
        args.cancelAfter === null ? null : asSafeNumber(args.cancelAfter, "cancel after"),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcEscrowCreate",
    "escrow_create",
  );
}

export async function buildEscrowFinish(
  args: SimpleArgs & { escrowId: Uint8Array },
): Promise<BuiltEnvelope> {
  const submitter = ownerOf(args.account);
  const preimage = typedPreimage(
    "agora-trident-drc-escrow-finish-v1",
    "drc_escrow_finish",
    args.chainId,
    args.genesis,
    [u32(1), submitter, args.escrowId, u64(args.fee), u64(args.nonce)],
  );
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      submitter: jsonBytes(submitter),
      escrow_id: jsonBytes(args.escrowId),
      fee: feeAmount(args.fee),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcEscrowFinish",
    "escrow_finish",
  );
}

export async function buildEscrowCancel(
  args: SimpleArgs & { escrowId: Uint8Array },
): Promise<BuiltEnvelope> {
  const submitter = ownerOf(args.account);
  const preimage = typedPreimage(
    "agora-trident-drc-escrow-cancel-v1",
    "drc_escrow_cancel",
    args.chainId,
    args.genesis,
    [u32(1), submitter, args.escrowId, u64(args.fee), u64(args.nonce)],
  );
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      submitter: jsonBytes(submitter),
      escrow_id: jsonBytes(args.escrowId),
      fee: feeAmount(args.fee),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcEscrowCancel",
    "escrow_cancel",
  );
}

export async function buildCheckCreate(
  args: SimpleArgs & {
    destination: Uint8Array;
    amount: bigint;
    destinationTag: number | null;
    sourceTag: number | null;
    expiresAfter: bigint | null;
  },
): Promise<BuiltEnvelope> {
  const owner = ownerOf(args.account);
  const preimage = typedPreimage(
    "agora-trident-drc-check-create-v1",
    "drc_check_create",
    args.chainId,
    args.genesis,
    [
      u32(1),
      owner,
      args.destination,
      u64(args.amount),
      u64(args.fee),
      ...tagPair(args.destinationTag, args.sourceTag),
      new Uint8Array(32),
      optionBytes(args.expiresAfter === null ? null : u64(args.expiresAfter)),
      u64(args.nonce),
    ],
  );
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      owner: jsonBytes(owner),
      destination: jsonBytes(args.destination),
      amount: asSafeNumber(args.amount, "amount"),
      fee: feeAmount(args.fee),
      destination_tag: args.destinationTag,
      source_tag: args.sourceTag,
      invoice_id: jsonBytes(new Uint8Array(32)),
      expires_after_blue_score:
        args.expiresAfter === null ? null : asSafeNumber(args.expiresAfter, "expiry"),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcCheckCreate",
    "check_create",
  );
}

export async function buildCheckCash(
  args: SimpleArgs & { checkId: Uint8Array },
): Promise<BuiltEnvelope> {
  const submitter = ownerOf(args.account);
  const preimage = typedPreimage(
    "agora-trident-drc-check-cash-v1",
    "drc_check_cash",
    args.chainId,
    args.genesis,
    [u32(1), submitter, args.checkId, u64(args.fee), u64(args.nonce)],
  );
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      submitter: jsonBytes(submitter),
      check_id: jsonBytes(args.checkId),
      fee: feeAmount(args.fee),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcCheckCash",
    "check_cash",
  );
}

export async function buildCheckCancel(
  args: SimpleArgs & { checkId: Uint8Array },
): Promise<BuiltEnvelope> {
  const submitter = ownerOf(args.account);
  const preimage = typedPreimage(
    "agora-trident-drc-check-cancel-v1",
    "drc_check_cancel",
    args.chainId,
    args.genesis,
    [u32(1), submitter, args.checkId, u64(args.fee), u64(args.nonce)],
  );
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      submitter: jsonBytes(submitter),
      check_id: jsonBytes(args.checkId),
      fee: feeAmount(args.fee),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcCheckCancel",
    "check_cancel",
  );
}

export async function buildChannelCreate(
  args: SimpleArgs & {
    destination: Uint8Array;
    amount: bigint;
    claimPublicKey: Uint8Array;
    settleDelay: bigint;
    destinationTag: number | null;
    sourceTag: number | null;
    cancelAfter: bigint | null;
  },
): Promise<BuiltEnvelope> {
  if (args.claimPublicKey.length !== 33) throw new Error("claim public key must be 33 bytes");
  if (args.settleDelay <= 0n) throw new Error("settle delay must be positive");
  const owner = ownerOf(args.account);
  const preimage = typedPreimage(
    "agora-trident-drc-payment-channel-create-v1",
    "drc_payment_channel_create",
    args.chainId,
    args.genesis,
    [
      u32(1),
      owner,
      args.destination,
      u64(args.amount),
      u64(args.fee),
      borshBytes(args.claimPublicKey),
      u64(args.settleDelay),
      ...tagPair(args.destinationTag, args.sourceTag),
      new Uint8Array(32),
      optionBytes(args.cancelAfter === null ? null : u64(args.cancelAfter)),
      u64(args.nonce),
    ],
  );
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      owner: jsonBytes(owner),
      destination: jsonBytes(args.destination),
      amount: asSafeNumber(args.amount, "amount"),
      fee: feeAmount(args.fee),
      claim_public_key: jsonBytes(args.claimPublicKey),
      settle_delay_blue_scores: asSafeNumber(args.settleDelay, "settle delay"),
      destination_tag: args.destinationTag,
      source_tag: args.sourceTag,
      invoice_id: jsonBytes(new Uint8Array(32)),
      cancel_after_blue_score:
        args.cancelAfter === null ? null : asSafeNumber(args.cancelAfter, "cancel after"),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcPaymentChannelCreate",
    "payment_channel_create",
  );
}

export async function buildChannelFund(
  args: SimpleArgs & { channelId: Uint8Array; amount: bigint },
): Promise<BuiltEnvelope> {
  const submitter = ownerOf(args.account);
  const preimage = typedPreimage(
    "agora-trident-drc-payment-channel-fund-v1",
    "drc_payment_channel_fund",
    args.chainId,
    args.genesis,
    [u32(1), submitter, args.channelId, u64(args.amount), u64(args.fee), u64(args.nonce)],
  );
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      submitter: jsonBytes(submitter),
      channel_id: jsonBytes(args.channelId),
      amount: asSafeNumber(args.amount, "amount"),
      fee: feeAmount(args.fee),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcPaymentChannelFund",
    "payment_channel_fund",
  );
}

export function offledgerClaimPreimage(args: {
  channelId: Uint8Array;
  cumulative: bigint;
  chainId: string;
  genesis: Uint8Array;
}): Uint8Array {
  return bound(
    "agora-trident-drc-payment-channel-offledger-claim-v1",
    args.chainId,
    args.genesis,
    [args.channelId, u64(args.cumulative)],
  );
}

export async function buildChannelClaim(
  args: SimpleArgs & {
    channelId: Uint8Array;
    cumulative: bigint;
    claimSecret: Uint8Array;
  },
): Promise<BuiltEnvelope> {
  const submitter = ownerOf(args.account);
  const claimSig = await signPreimage(
    args.claimSecret,
    offledgerClaimPreimage({
      channelId: args.channelId,
      cumulative: args.cumulative,
      chainId: args.chainId,
      genesis: args.genesis,
    }),
  );
  const preimage = typedPreimage(
    "agora-trident-drc-payment-channel-claim-v1",
    "drc_payment_channel_claim",
    args.chainId,
    args.genesis,
    [
      u32(1),
      submitter,
      args.channelId,
      u64(args.cumulative),
      borshBytes(claimSig.signature),
      u64(args.fee),
      u64(args.nonce),
    ],
  );
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      submitter: jsonBytes(submitter),
      channel_id: jsonBytes(args.channelId),
      cumulative_authorized: asSafeNumber(args.cumulative, "cumulative"),
      channel_claim_signature: jsonBytes(claimSig.signature),
      fee: feeAmount(args.fee),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcPaymentChannelClaim",
    "payment_channel_claim",
  );
}

export async function buildChannelClose(
  args: SimpleArgs & {
    channelId: Uint8Array;
    closeKind: "OwnerScheduleClose" | "DestinationClose" | "Finalize";
  },
): Promise<BuiltEnvelope> {
  const discriminant =
    args.closeKind === "OwnerScheduleClose" ? 1 : args.closeKind === "DestinationClose" ? 2 : 3;
  const submitter = ownerOf(args.account);
  const preimage = typedPreimage(
    "agora-trident-drc-payment-channel-close-v1",
    "drc_payment_channel_close",
    args.chainId,
    args.genesis,
    [u32(1), submitter, args.channelId, u8(discriminant), u64(args.fee), u64(args.nonce)],
  );
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      submitter: jsonBytes(submitter),
      channel_id: jsonBytes(args.channelId),
      close_kind: args.closeKind,
      fee: feeAmount(args.fee),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcPaymentChannelClose",
    "payment_channel_close",
  );
}

export async function buildTrustLine(
  args: SimpleArgs & { issuer: Uint8Array; currency: Uint8Array; limit: bigint },
): Promise<BuiltEnvelope> {
  const holder = ownerOf(args.account);
  const preimage = trustLinePreimage({
    holder,
    issuer: args.issuer,
    currency: args.currency,
    limit: args.limit,
    fee: args.fee,
    nonce: args.nonce,
    chainId: args.chainId,
    genesis: args.genesis,
  });
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      holder: jsonBytes(holder),
      issuer: jsonBytes(args.issuer),
      currency: jsonBytes(args.currency),
      limit: asSafeNumber(args.limit, "limit"),
      fee: feeAmount(args.fee),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcTrustLineSet",
    "trust_line_set",
  );
}

export async function buildIssuedTransfer(
  args: SimpleArgs & {
    recipient: Uint8Array;
    issuer: Uint8Array;
    currency: Uint8Array;
    amount: bigint;
    destinationTag: number | null;
    sourceTag: number | null;
  },
): Promise<BuiltEnvelope> {
  const sender = ownerOf(args.account);
  const preimage = bound(
    "agora-trident-drc-issued-transfer-v1",
    args.chainId,
    args.genesis,
    [
      u32(1),
      sender,
      args.recipient,
      args.issuer,
      args.currency,
      u64(args.amount),
      u64(args.fee),
      ...tagPair(args.destinationTag, args.sourceTag),
      new Uint8Array(32),
      u64(args.nonce),
      u8(0),
    ],
  );
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      sender: jsonBytes(sender),
      recipient: jsonBytes(args.recipient),
      issuer: jsonBytes(args.issuer),
      currency: jsonBytes(args.currency),
      amount: asSafeNumber(args.amount, "amount"),
      fee: feeAmount(args.fee),
      destination_tag: args.destinationTag,
      source_tag: args.sourceTag,
      invoice_id: jsonBytes(new Uint8Array(32)),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcIssuedTransfer",
    "issued_transfer",
  );
}

const ISSUED_POLICY_WIRE = [
  "EnableRequireAuth",
  "EnableGlobalFreeze",
  "ClearGlobalFreeze",
  "EnableNoFreeze",
  "EnableClawback",
] as const;

export async function buildIssuedPolicy(
  args: SimpleArgs & { currency: Uint8Array; action: (typeof ISSUED_POLICY_WIRE)[number] },
): Promise<BuiltEnvelope> {
  const issuer = ownerOf(args.account);
  const action = ISSUED_POLICY_WIRE.indexOf(args.action);
  const preimage = issuedPolicyMessage({
    issuer,
    currency: args.currency,
    action,
    fee: args.fee,
    nonce: args.nonce,
    chainId: args.chainId,
    genesis: args.genesis,
  });
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      issuer: jsonBytes(issuer),
      currency: jsonBytes(args.currency),
      action: args.action,
      fee: feeAmount(args.fee),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcIssuedAssetPolicySet",
    "issued_asset_policy_set",
  );
}

export async function buildIssuerControl(
  args: SimpleArgs & {
    holder: Uint8Array;
    currency: Uint8Array;
    action: "AuthorizeHolder" | "SetLineFrozen" | "SetLineDeepFrozen";
    flag: boolean;
  },
): Promise<BuiltEnvelope> {
  const issuer = ownerOf(args.account);
  let actionBytes: Uint8Array;
  let actionJson: unknown;
  if (args.action === "AuthorizeHolder") {
    actionBytes = u8(0);
    actionJson = "AuthorizeHolder";
  } else if (args.action === "SetLineFrozen") {
    actionBytes = concat([u8(1), u8(args.flag ? 1 : 0)]);
    actionJson = { SetLineFrozen: args.flag };
  } else {
    actionBytes = concat([u8(2), u8(args.flag ? 1 : 0)]);
    actionJson = { SetLineDeepFrozen: args.flag };
  }
  const body = bound(
    "agora-trident-drc-trust-line-issuer-control-v1",
    args.chainId,
    args.genesis,
    [
      u32(1),
      issuer,
      args.holder,
      args.currency,
      actionBytes,
      u64(args.fee),
      u64(args.nonce),
      u8(0),
    ],
  );
  return signEnvelope(
    args.account,
    sha256(body),
    {
      version: 1,
      issuer: jsonBytes(issuer),
      holder: jsonBytes(args.holder),
      currency: jsonBytes(args.currency),
      action: actionJson,
      fee: feeAmount(args.fee),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcTrustLineIssuerControl",
    "trust_line_issuer_control",
  );
}

export async function buildClawback(
  args: SimpleArgs & { holder: Uint8Array; currency: Uint8Array; amount: bigint },
): Promise<BuiltEnvelope> {
  const issuer = ownerOf(args.account);
  const body = bound("agora-trident-drc-issued-clawback-v1", args.chainId, args.genesis, [
    u32(1),
    issuer,
    args.holder,
    args.currency,
    u64(args.amount),
    u64(args.fee),
    u64(args.nonce),
    u8(0),
  ]);
  return signEnvelope(
    args.account,
    sha256(body),
    {
      version: 1,
      issuer: jsonBytes(issuer),
      holder: jsonBytes(args.holder),
      currency: jsonBytes(args.currency),
      amount: asSafeNumber(args.amount, "amount"),
      fee: feeAmount(args.fee),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitDrcIssuedClawback",
    "issued_clawback",
  );
}

export async function buildStakeBond(args: {
  account: WalletAccount;
  asset: "OVL" | "DRC";
  amount: bigint;
  withdrawal: Uint8Array;
  commissionBps: number;
  nonce: bigint;
  chainId: string;
  genesis: Uint8Array;
}): Promise<BuiltEnvelope> {
  const actor = ownerOf(args.account);
  const consensus = args.account.publicKey;
  if (consensus.length !== 33) throw new Error("consensus pubkey must be 33 bytes");
  const preimage = stakeBondPreimage({
    asset: args.asset,
    actor,
    amount: args.amount,
    consensusPubkey: consensus,
    withdrawal: args.withdrawal,
    commissionBps: args.commissionBps,
    nonce: args.nonce,
    chainId: args.chainId,
    genesis: args.genesis,
  });
  return signEnvelope(
    args.account,
    preimage,
    {
      version: 1,
      asset: args.asset,
      kind: "Bond",
      actor: jsonBytes(actor),
      validator: jsonBytes(actor),
      amount: asSafeNumber(args.amount, "amount"),
      consensus_pubkey: jsonBytes(consensus),
      withdrawal: jsonBytes(args.withdrawal),
      commission_bps: args.commissionBps,
      metadata_hash: jsonBytes(new Uint8Array(32)),
      nonce: asSafeNumber(args.nonce, "nonce"),
    },
    "agora_submitStakeTx",
    "stake_tx",
  );
}
