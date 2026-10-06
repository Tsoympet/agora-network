/**
 * Locked against `agora-types` light witness vectors.
 * Run: `node --experimental-strip-types light-client/agora-light.test.ts`
 */
import assert from "node:assert/strict";

import {
  assertExpectedNetwork,
  compareHeaderSpines,
  foldBodyBinding,
  hashLightHeader,
  readNativeBalances,
  verifyDrcObjectHeaderProof,
  verifyIncomingTlt,
  verifyLaneId,
  verifyReportedFinality,
  verifySelectedParentSpine,
  type LightHeaderFields,
  type LightHeaderRecord,
} from "./agoraLight.ts";
import { keyValueVault } from "./vault.ts";

const merkle = "1864d005427479e7b35c49f49342edc1d152d7c9924e88b74ee63d400a8156f4";
const header: LightHeaderFields = {
  version: 1,
  parents: [],
  timestamp_ms: 1_700_000_000_000,
  bits: 1,
  nonce: 7,
  tx_root: merkle,
};
assert.equal(
  hashLightHeader(header),
  "0687bb9723213df52b4bc9868c70c99c0e6bc21758a81ec158cd35338ae582be",
);
assert.equal(
  foldBodyBinding(merkle, [
    {
      kind: "v2",
      account_ids: ["633dea78d31691edd0c36d6dfe67ee8c0eaef738a86a71e6fd9ef698a9aadf91"],
      stake_ids: [],
    },
  ]),
  "4d339b0ad0130a65e06ce1434ccad358db4c50e92eb952ab5d0a0aad008da375",
);
assert.equal(
  foldBodyBinding(merkle, [
    {
      kind: "versioned",
      domain: "agora-block-body-v5",
      version: 5,
      id_lists: [["02".repeat(32)]],
    },
  ]),
  "22d7f391c609b5798f27122b4eab779d2416098b8fd25d740d18f4f2180d7585",
);
assert.throws(
  () =>
    foldBodyBinding(merkle, [
      { kind: "versioned", domain: "agora-block-body-v5", version: 5, id_lists: [] },
    ]),
  /unsupported body binding/,
);

function record(
  fields: LightHeaderFields,
  selectedParent: string | null,
  blueScore: number,
): LightHeaderRecord {
  return {
    hash: hashLightHeader(fields),
    selected_parent: selectedParent,
    blue_score: blueScore,
    header: fields,
  };
}

const genesis = record(
  { version: 1, parents: [], timestamp_ms: 1, bits: 1, nonce: 1, tx_root: "11".repeat(32) },
  null,
  1,
);
const childFields: LightHeaderFields = {
  version: 1,
  parents: [genesis.hash],
  timestamp_ms: 2,
  bits: 1,
  nonce: 2,
  tx_root: "22".repeat(32),
};
const child = record(childFields, genesis.hash, 2);
const tip = record(
  {
    version: 1,
    parents: [child.hash],
    timestamp_ms: 3,
    bits: 1,
    nonce: 3,
    tx_root: "33".repeat(32),
  },
  child.hash,
  3,
);
const spine = [tip, child, genesis];
assert.equal(verifySelectedParentSpine(spine, genesis.hash), tip.hash);
assert.throws(() => verifySelectedParentSpine(spine, "ab".repeat(32)), /wrong network genesis/);
const broken = [{ ...tip, selected_parent: "44".repeat(32) }, child, genesis];
assert.throws(() => verifySelectedParentSpine(broken, genesis.hash), /ancestry mismatch/);
const flat = [{ ...tip, blue_score: 1 }, child, genesis];
assert.throws(() => verifySelectedParentSpine(flat, genesis.hash), /blue score/);
assert.equal(compareHeaderSpines(spine, spine), "same");
const reorgTip = record(
  {
    version: 1,
    parents: [child.hash],
    timestamp_ms: 9,
    bits: 1,
    nonce: 9,
    tx_root: "99".repeat(32),
  },
  child.hash,
  4,
);
assert.equal(compareHeaderSpines(spine, [reorgTip, child, genesis]), "reorg");
assert.equal(
  compareHeaderSpines(spine, [{ ...child, blue_score: 8 }, genesis]),
  "mismatch",
);
assert.throws(() => assertExpectedNetwork("dev", "testnet"), /wrong network/);
assert.doesNotThrow(() => assertExpectedNetwork("Agora-Dev", "dev"));

assert.throws(
  () =>
    verifyIncomingTlt({
      tx_id: merkle,
      status: "unknown",
      on_virtual_spine: false,
      proof: null,
    }),
  /missing TLT inclusion proof/,
);

const inclusionHeader = { ...header, tx_root: merkle };
const proof = {
  hash: hashLightHeader(inclusionHeader),
  header: inclusionHeader,
  tx_merkle_root: merkle,
  utxo_only: true,
  binding: [],
  inclusion: { index: 0, tx_id: merkle, siblings: [] },
  programmable_lane: "OVL",
  drc_contract_lane: false,
};
assert.throws(
  () =>
    verifyIncomingTlt({
      tx_id: merkle,
      status: "pending",
      on_virtual_spine: false,
      proof,
    }),
  /not confirmed/,
);
const verified = verifyIncomingTlt({
  tx_id: merkle,
  status: "confirmed",
  on_virtual_spine: true,
  proof,
});
assert.equal(verified.txMerkleRoot, merkle);
assert.throws(
  () =>
    verifyIncomingTlt({
      tx_id: merkle,
      status: "confirmed",
      on_virtual_spine: true,
      proof: {
        ...proof,
        inclusion: { index: 0, tx_id: merkle, siblings: ["00".repeat(32)] },
      },
    }),
  /merkle inclusion failed/,
);
assert.throws(
  () => verifyLaneId(proof, "aa".repeat(32)),
  /not in the header binding/,
);
const bound = {
  ...proof,
  header: {
    ...inclusionHeader,
    tx_root: "4d339b0ad0130a65e06ce1434ccad358db4c50e92eb952ab5d0a0aad008da375",
  },
  binding: [
    {
      kind: "v3" as const,
      execution_ids: ["633dea78d31691edd0c36d6dfe67ee8c0eaef738a86a71e6fd9ef698a9aadf91"],
    },
  ],
};
bound.hash = hashLightHeader(bound.header);
assert.throws(() => verifyLaneId(bound, bound.binding[0].execution_ids[0]), /tx_root/);
const executionId = "ab".repeat(32);
const executionRoot = foldBodyBinding(merkle, [{ kind: "v3", execution_ids: [executionId] }]);
const laneHeader = { ...inclusionHeader, tx_root: executionRoot };
verifyLaneId(
  {
    ...proof,
    hash: hashLightHeader(laneHeader),
    header: laneHeader,
    binding: [{ kind: "v3", execution_ids: [executionId] }],
  },
  executionId,
);
assert.throws(
  () =>
    verifyLaneId(
      {
        ...proof,
        hash: hashLightHeader(laneHeader),
        header: laneHeader,
        binding: [{ kind: "v3", execution_ids: [executionId] }],
      },
      "cd".repeat(32),
    ),
  /not in the header binding/,
);

assert.throws(() => verifyDrcObjectHeaderProof({ object: { kind: "escrow" } }), /missing DRC/);
assert.throws(
  () =>
    verifyReportedFinality(
      {
        block_hash: tip.hash,
        pow_work_met: true,
        stake_fields_present: false,
      },
      child.hash,
    ),
  /finality header mismatch/,
);
assert.throws(
  () =>
    verifyReportedFinality(
      {
        block_hash: tip.hash,
        pow_work_met: true,
        stake_fields_present: false,
      },
      tip.hash,
    ),
  /missing quorum totals/,
);
assert.equal(
  verifyReportedFinality(
    {
      block_hash: tip.hash,
      pow_work_met: true,
      stake_fields_present: true,
      ovl_signed_stake: "2",
      ovl_active_stake: "3",
      drc_signed_stake: "1",
      drc_active_stake: "3",
    },
    tip.hash,
  ),
  "AwaitingDrcQuorum",
);

const balances = readNativeBalances({
  address: "agora1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqug5w4g",
  address_hex: "00".repeat(20),
  assets: {
    TLT: { asset: "TLT", module: "utxo", balance: "0", header_proven: false },
    OVL: {
      asset: "OVL",
      module: "account",
      balance: "50",
      nonce: "0",
      record: "present",
      header_proven: false,
    },
    DRC: {
      asset: "DRC",
      module: "account",
      balance: "0",
      nonce: "0",
      record: "absent",
      header_proven: false,
    },
  },
});
assert.equal(balances.assets.OVL.balance, "50");
assert.notEqual(balances.assets.TLT.module, balances.assets.DRC.module);
assert.throws(
  () =>
    readNativeBalances({
      ...balances,
      assets: {
        ...balances.assets,
        DRC: { ...balances.assets.DRC, header_proven: true as unknown as false },
      },
    }),
  /header-proven/,
);

const secure = new Map<string, string>();
const adapter = keyValueVault({
  getItemAsync: async (key) => secure.get(key) ?? null,
  setItemAsync: async (key, value) => {
    secure.set(key, value);
  },
  deleteItemAsync: async (key) => {
    secure.delete(key);
  },
});
await adapter.save("sealed");
assert.equal(await adapter.load(), "sealed");
await adapter.clear();
assert.equal(await adapter.load(), null);

console.log("agora light client checks ok");
