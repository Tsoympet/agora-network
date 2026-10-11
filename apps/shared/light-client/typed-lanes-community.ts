/**
 * Device-local signed Hub / Grant / Mission registrations.
 * Keys stay in the Agora BIP-44 vault.
 */

import { addressHrpForNetwork, encodeAddress } from "./address.ts";
import type { LightClient } from "./rpc.ts";
import {
  accountFromMnemonic,
  bytesToHex,
  concat,
  encodeString,
  hexToBytes,
  jsonAddress,
  jsonBytes,
  nodeBinding,
  parseRecipient,
  requireAddress,
  requireHash,
  signBound,
  u32,
  u64,
  u8,
  type BuiltTypedEnvelope,
} from "./typed-lanes.ts";
import { chainIdForNetwork } from "./wallet.ts";

export const HUB_REGISTRATION_DOMAIN = new TextEncoder().encode(
  "agora-hub-registration-v1",
);
export const GRANT_REGISTRATION_DOMAIN = new TextEncoder().encode(
  "agora-grant-registration-v1",
);
export const MISSION_REGISTRATION_DOMAIN = new TextEncoder().encode(
  "agora-mission-registration-v1",
);

export const COMMUNITY_GRANT_KINDS = [
  "Micro",
  "Milestone",
  "Bounty",
  "Retroactive",
] as const;
export type CommunityGrantKindName = (typeof COMMUNITY_GRANT_KINDS)[number];

export const TREASURY_IDS = {
  TltSecurity: 0,
  OvlBuilder: 1,
  DrcCommunity: 2,
} as const;
export type TreasuryIdName = keyof typeof TREASURY_IDS;

export function communityGrantKindDiscriminant(
  kind: CommunityGrantKindName,
): number {
  const index = COMMUNITY_GRANT_KINDS.indexOf(kind);
  if (index < 0) throw new Error(`unknown grant kind ${kind}`);
  return index;
}

function encodeAddressVec(addresses: Uint8Array[]): Uint8Array {
  return concat([u32(addresses.length), ...addresses.map(requireAddress)]);
}

export function encodeHubRegistrationBody(fields: {
  version: number;
  publicName: string;
  classification: string;
  charterHash: Uint8Array;
  coordinators: Uint8Array[];
  treasuryMultisig: Uint8Array;
  electionTermEpochs: number | bigint;
  reportingIntervalEpochs: number | bigint;
  coiDisclosureRoot: Uint8Array;
  deliverablesRoot: Uint8Array;
  accreditationProposalId: number | bigint;
  nonce: number | bigint;
}): Uint8Array {
  return concat([
    u32(fields.version),
    encodeString(fields.publicName),
    encodeString(fields.classification),
    requireHash(fields.charterHash),
    encodeAddressVec(fields.coordinators),
    requireAddress(fields.treasuryMultisig),
    u64(fields.electionTermEpochs),
    u64(fields.reportingIntervalEpochs),
    requireHash(fields.coiDisclosureRoot),
    requireHash(fields.deliverablesRoot),
    u64(fields.accreditationProposalId),
    u64(fields.nonce),
  ]);
}

export function encodeGrantRegistrationBody(fields: {
  version: number;
  registrar: Uint8Array;
  proposalId: number | bigint;
  treasury: TreasuryIdName;
  beneficiary: Uint8Array;
  total: number | bigint;
  kind: CommunityGrantKindName;
  milestones: Array<{
    index: number;
    amount: number | bigint;
    deliverableHash: Uint8Array;
  }>;
  coiDisclosureHash: Uint8Array;
  nonce: number | bigint;
}): Uint8Array {
  const milestones = concat([
    u32(fields.milestones.length),
    ...fields.milestones.flatMap((milestone) => [
      u32(milestone.index),
      u64(milestone.amount),
      requireHash(milestone.deliverableHash),
    ]),
  ]);
  return concat([
    u32(fields.version),
    requireAddress(fields.registrar),
    u64(fields.proposalId),
    u8(TREASURY_IDS[fields.treasury]),
    requireAddress(fields.beneficiary),
    u64(fields.total),
    u8(communityGrantKindDiscriminant(fields.kind)),
    milestones,
    requireHash(fields.coiDisclosureHash),
    u64(fields.nonce),
  ]);
}

export function encodeMissionRegistrationBody(fields: {
  version: number;
  sponsor: Uint8Array;
  rewardTreasury: TreasuryIdName;
  reward: number | bigint;
  requirementsHash: Uint8Array;
  nonce: number | bigint;
}): Uint8Array {
  return concat([
    u32(fields.version),
    requireAddress(fields.sponsor),
    u8(TREASURY_IDS[fields.rewardTreasury]),
    u64(fields.reward),
    requireHash(fields.requirementsHash),
    u64(fields.nonce),
  ]);
}

export async function buildSignedHubRegistration(options: {
  mnemonic: string;
  accountIndex?: number;
  network?: string;
  genesisHash: string;
  chainId?: string;
  publicName: string;
  classification: string;
  charterHash: string;
  coordinators?: string[];
  treasuryMultisig: string;
  electionTermEpochs: number;
  reportingIntervalEpochs: number;
  coiDisclosureRoot: string;
  deliverablesRoot: string;
  accreditationProposalId: number;
  nonce: number;
}): Promise<BuiltTypedEnvelope> {
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(
    options.mnemonic,
    network,
    options.accountIndex ?? 0,
  );
  const coordinator = hexToBytes(account.addressHex);
  const coordinators = (options.coordinators ?? [account.addressHex]).map(
    (value) => parseRecipient(value, network).bytes,
  );
  const treasury = parseRecipient(options.treasuryMultisig, network).bytes;
  const chainId = options.chainId ?? chainIdForNetwork(network);
  const body = encodeHubRegistrationBody({
    version: 1,
    publicName: options.publicName,
    classification: options.classification,
    charterHash: requireHash(hexToBytes(options.charterHash)),
    coordinators,
    treasuryMultisig: treasury,
    electionTermEpochs: options.electionTermEpochs,
    reportingIntervalEpochs: options.reportingIntervalEpochs,
    coiDisclosureRoot: requireHash(hexToBytes(options.coiDisclosureRoot)),
    deliverablesRoot: requireHash(hexToBytes(options.deliverablesRoot)),
    accreditationProposalId: options.accreditationProposalId,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    HUB_REGISTRATION_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  const tx = {
    version: 1,
    public_name: options.publicName,
    classification: options.classification,
    charter_hash: jsonBytes(requireHash(hexToBytes(options.charterHash))),
    coordinators: coordinators.map(jsonAddress),
    treasury_multisig: jsonAddress(treasury),
    election_term_epochs: options.electionTermEpochs,
    reporting_interval_epochs: options.reportingIntervalEpochs,
    coi_disclosure_root: jsonBytes(requireHash(hexToBytes(options.coiDisclosureRoot))),
    deliverables_root: jsonBytes(requireHash(hexToBytes(options.deliverablesRoot))),
    accreditation_proposal_id: options.accreditationProposalId,
    nonce: options.nonce,
    public_key: jsonBytes(signed.publicKey),
    signature: jsonBytes(signed.signature),
  };
  return {
    tx,
    from: account.addressHex,
    fromBech32: account.addressBech32,
    to: bytesToHex(coordinator),
    toBech32: encodeAddress(bytesToHex(coordinator), addressHrpForNetwork(network)),
    amount: 0,
    fee: 0,
    signingBytes: signed.signingBytes,
  };
}

export async function sendHubRegistration(
  client: LightClient,
  options: {
    mnemonic: string;
    accountIndex?: number;
    network?: string;
    publicName: string;
    classification: string;
    charterHash: string;
    coordinators?: string[];
    treasuryMultisig: string;
    electionTermEpochs: number;
    reportingIntervalEpochs: number;
    coiDisclosureRoot: string;
    deliverablesRoot: string;
    accreditationProposalId: number;
    nonce?: number;
  },
): Promise<{ id: string; built: BuiltTypedEnvelope }> {
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(
    options.mnemonic,
    network,
    options.accountIndex ?? 0,
  );
  const bind = await nodeBinding(client, network);
  const nonce =
    options.nonce ??
    (await client.getHubCoordinatorNonce(account.addressBech32)).nonce;
  const built = await buildSignedHubRegistration({
    ...options,
    nonce,
    genesisHash: bind.genesisHash,
    chainId: bind.chainId,
    network: bind.network,
  });
  const submitted = await client.submitHubRegistration(built.tx);
  return { id: submitted.registration_id, built };
}

export async function buildSignedGrantRegistration(options: {
  mnemonic: string;
  accountIndex?: number;
  network?: string;
  genesisHash: string;
  chainId?: string;
  proposalId: number;
  treasury: TreasuryIdName;
  beneficiary: string;
  total: number;
  kind: CommunityGrantKindName;
  milestones?: Array<{
    index: number;
    amount: number;
    deliverableHash: string;
  }>;
  coiDisclosureHash?: string;
  nonce: number;
}): Promise<BuiltTypedEnvelope> {
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(
    options.mnemonic,
    network,
    options.accountIndex ?? 0,
  );
  const registrar = hexToBytes(account.addressHex);
  const beneficiary = parseRecipient(options.beneficiary, network).bytes;
  const chainId = options.chainId ?? chainIdForNetwork(network);
  const milestones = (options.milestones ?? []).map((milestone) => ({
    index: milestone.index,
    amount: milestone.amount,
    deliverableHash: requireHash(hexToBytes(milestone.deliverableHash)),
  }));
  const coi = requireHash(hexToBytes(options.coiDisclosureHash ?? "00".repeat(32)));
  const body = encodeGrantRegistrationBody({
    version: 1,
    registrar,
    proposalId: options.proposalId,
    treasury: options.treasury,
    beneficiary,
    total: options.total,
    kind: options.kind,
    milestones,
    coiDisclosureHash: coi,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    GRANT_REGISTRATION_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  const tx = {
    version: 1,
    registrar: jsonAddress(registrar),
    proposal_id: options.proposalId,
    treasury: options.treasury,
    beneficiary: jsonAddress(beneficiary),
    total: options.total,
    kind: options.kind,
    milestones: milestones.map((milestone) => ({
      index: milestone.index,
      amount: milestone.amount,
      deliverable_hash: jsonBytes(milestone.deliverableHash),
    })),
    coi_disclosure_hash: jsonBytes(coi),
    nonce: options.nonce,
    public_key: jsonBytes(signed.publicKey),
    signature: jsonBytes(signed.signature),
  };
  return {
    tx,
    from: account.addressHex,
    fromBech32: account.addressBech32,
    to: bytesToHex(beneficiary),
    toBech32: encodeAddress(bytesToHex(beneficiary), addressHrpForNetwork(network)),
    amount: options.total,
    fee: 0,
    signingBytes: signed.signingBytes,
  };
}

export async function sendGrantRegistration(
  client: LightClient,
  options: {
    mnemonic: string;
    accountIndex?: number;
    network?: string;
    proposalId: number;
    treasury: TreasuryIdName;
    beneficiary: string;
    total: number;
    kind: CommunityGrantKindName;
    milestones?: Array<{
      index: number;
      amount: number;
      deliverableHash: string;
    }>;
    coiDisclosureHash?: string;
    nonce?: number;
  },
): Promise<{ id: string; built: BuiltTypedEnvelope }> {
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(
    options.mnemonic,
    network,
    options.accountIndex ?? 0,
  );
  const bind = await nodeBinding(client, network);
  const nonce =
    options.nonce ??
    (await client.getGrantRegistrarNonce(account.addressBech32)).nonce;
  const built = await buildSignedGrantRegistration({
    ...options,
    nonce,
    genesisHash: bind.genesisHash,
    chainId: bind.chainId,
    network: bind.network,
  });
  const submitted = await client.submitGrantRegistration(built.tx);
  return { id: submitted.registration_id, built };
}

export async function buildSignedMissionRegistration(options: {
  mnemonic: string;
  accountIndex?: number;
  network?: string;
  genesisHash: string;
  chainId?: string;
  rewardTreasury: TreasuryIdName;
  reward: number;
  requirementsHash: string;
  nonce: number;
}): Promise<BuiltTypedEnvelope> {
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(
    options.mnemonic,
    network,
    options.accountIndex ?? 0,
  );
  const sponsor = hexToBytes(account.addressHex);
  const chainId = options.chainId ?? chainIdForNetwork(network);
  const requirements = requireHash(hexToBytes(options.requirementsHash));
  const body = encodeMissionRegistrationBody({
    version: 1,
    sponsor,
    rewardTreasury: options.rewardTreasury,
    reward: options.reward,
    requirementsHash: requirements,
    nonce: options.nonce,
  });
  const signed = await signBound(
    account.secretKey,
    MISSION_REGISTRATION_DOMAIN,
    chainId,
    options.genesisHash,
    body,
  );
  const tx = {
    version: 1,
    sponsor: jsonAddress(sponsor),
    reward_treasury: options.rewardTreasury,
    reward: options.reward,
    requirements_hash: jsonBytes(requirements),
    nonce: options.nonce,
    public_key: jsonBytes(signed.publicKey),
    signature: jsonBytes(signed.signature),
  };
  return {
    tx,
    from: account.addressHex,
    fromBech32: account.addressBech32,
    to: account.addressHex,
    toBech32: account.addressBech32,
    amount: options.reward,
    fee: 0,
    signingBytes: signed.signingBytes,
  };
}

export async function sendMissionRegistration(
  client: LightClient,
  options: {
    mnemonic: string;
    accountIndex?: number;
    network?: string;
    rewardTreasury: TreasuryIdName;
    reward: number;
    requirementsHash: string;
    nonce?: number;
  },
): Promise<{ id: string; built: BuiltTypedEnvelope }> {
  const network = options.network ?? "mainnet";
  const account = accountFromMnemonic(
    options.mnemonic,
    network,
    options.accountIndex ?? 0,
  );
  const bind = await nodeBinding(client, network);
  const nonce =
    options.nonce ??
    (await client.getMissionSponsorNonce(account.addressBech32)).nonce;
  const built = await buildSignedMissionRegistration({
    ...options,
    nonce,
    genesisHash: bind.genesisHash,
    chainId: bind.chainId,
    network: bind.network,
  });
  const submitted = await client.submitMissionRegistration(built.tx);
  return { id: submitted.registration_id, built };
}
