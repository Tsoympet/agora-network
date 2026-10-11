/**
 * Device-local Trident checkpoint attestation signer.
 *
 * Matches Rust `CheckpointBody::signing_bytes` =
 * borsh(b"agora-trident-checkpoint-v1", body). Keys stay on the device.
 */

import { addressHrpForNetwork, encodeAddress } from "./address.ts";
import type { LightClient, LightFinalityBody } from "./rpc.ts";
import {
  bytesToHex,
  concat,
  encodeString,
  encodeVec,
  hexToBytes,
  u64,
} from "./typed-lanes.ts";
import { signTransactionBody } from "./wallet.ts";
import { accountFromSecretHex, type WalletAccount } from "./wallet.ts";

export const CHECKPOINT_ATTESTATION_DOMAIN = new TextEncoder().encode(
  "agora-trident-checkpoint-v1",
);

export type CheckpointBodyFields = {
  chainId: string;
  genesisHash: string;
  consensusPolicyHash: string;
  stateTransitionVersion: string;
  blueScore: number | bigint;
  blockHash: string;
  stateRoot: string;
  validatorEpoch: number | bigint;
};

export function encodeCheckpointBody(fields: CheckpointBodyFields): Uint8Array {
  const genesis = hexToBytes(fields.genesisHash);
  const policy = hexToBytes(fields.consensusPolicyHash);
  const block = hexToBytes(fields.blockHash);
  const stateRoot = hexToBytes(fields.stateRoot);
  if (
    genesis.length !== 32 ||
    policy.length !== 32 ||
    block.length !== 32 ||
    stateRoot.length !== 32
  ) {
    throw new Error("checkpoint hashes must be 32 bytes");
  }
  return concat([
    encodeString(fields.chainId),
    genesis,
    policy,
    encodeString(fields.stateTransitionVersion),
    u64(fields.blueScore),
    block,
    stateRoot,
    u64(fields.validatorEpoch),
  ]);
}

export function encodeCheckpointSigningBytes(
  fields: CheckpointBodyFields,
): Uint8Array {
  return concat([
    encodeVec(CHECKPOINT_ATTESTATION_DOMAIN),
    encodeCheckpointBody(fields),
  ]);
}

export function checkpointBodyFromRpc(
  body: LightFinalityBody,
): CheckpointBodyFields {
  return {
    chainId: body.chain_id,
    genesisHash: body.genesis_hash,
    consensusPolicyHash: body.consensus_policy_hash,
    stateTransitionVersion: body.state_transition_version,
    blueScore: body.blue_score,
    blockHash: body.block_hash,
    stateRoot: body.state_root,
    validatorEpoch: body.validator_epoch,
  };
}

function hexToByteArray(hex: string): number[] {
  return Array.from(hexToBytes(hex));
}

export async function buildSignedCheckpointAttestation(options: {
  body: CheckpointBodyFields;
  set: "OVL" | "DRC";
  account: WalletAccount;
}): Promise<{
  attestation: Record<string, unknown>;
  signingBytes: Uint8Array;
}> {
  if (options.set === "TLT" as string) {
    throw new Error("checkpoint attestations are OVL or DRC only");
  }
  const signingBytes = encodeCheckpointSigningBytes(options.body);
  const { publicKey, signature } = await signTransactionBody(
    options.account.secretKey,
    signingBytes,
  );
  const genesis = hexToByteArray(options.body.genesisHash);
  const policy = hexToByteArray(options.body.consensusPolicyHash);
  const block = hexToByteArray(options.body.blockHash);
  const stateRoot = hexToByteArray(options.body.stateRoot);
  return {
    signingBytes,
    attestation: {
      body: {
        chain_id: options.body.chainId,
        genesis_hash: genesis,
        consensus_policy_hash: policy,
        state_transition_version: options.body.stateTransitionVersion,
        blue_score: Number(options.body.blueScore),
        block_hash: block,
        state_root: stateRoot,
        validator_epoch: Number(options.body.validatorEpoch),
      },
      set: options.set,
      validator: hexToByteArray(options.account.addressHex),
      public_key: Array.from(publicKey),
      signature: Array.from(signature),
    },
  };
}

export async function sendCheckpointAttestation(
  client: LightClient,
  options: {
    secretHex: string;
    set: "OVL" | "DRC";
    blockHash?: string;
    network?: string;
  },
): Promise<{
  block_hash: string;
  state: string;
  finalized: boolean;
  signingBytesHex: string;
}> {
  const tips = options.blockHash
    ? [options.blockHash]
    : await client.getDagTips();
  if (!tips.length) {
    throw new Error("no DAG tip to attest");
  }
  const finality = await client.getFinality(tips[0]);
  if (!finality.body) {
    throw new Error("agora_getFinality did not return a checkpoint body");
  }
  const network = options.network ?? "testnet";
  const account = accountFromSecretHex(options.secretHex, network);
  const { attestation, signingBytes } = await buildSignedCheckpointAttestation({
    body: checkpointBodyFromRpc(finality.body),
    set: options.set,
    account,
  });
  const result = await client.submitAttestation(attestation);
  return {
    ...result,
    signingBytesHex: bytesToHex(signingBytes),
  };
}

export function experimentalValidatorAddress(
  secretHex: string,
  network = "testnet",
): { addressHex: string; addressBech32: string } {
  const account = accountFromSecretHex(secretHex, network);
  return {
    addressHex: account.addressHex,
    addressBech32: encodeAddress(
      account.addressHex,
      addressHrpForNetwork(network),
    ),
  };
}
