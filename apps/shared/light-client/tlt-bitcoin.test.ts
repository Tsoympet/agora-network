/**
 * Vectors shared with the Rust TLT coin selection, Merkle, and light-finality tests.
 * Run: `node --experimental-strip-types light-client/tlt-bitcoin.test.ts`
 */
import assert from "node:assert/strict";

import { selectTltCoins } from "./coinselect.ts";
import {
  proveTltTxMerkle,
  tltTxMerkleRoot,
  verifyTltTxMerkle,
  verifyTridentLight,
} from "./tltMerkle.ts";

const coins = [
  { tx_id: "aa".repeat(32), index: 0, value: 5 },
  { tx_id: "bb".repeat(32), index: 0, value: 4 },
  { tx_id: "cc".repeat(32), index: 1, value: 3 },
  { tx_id: "dd".repeat(32), index: 0, value: 1 },
];
const selected = selectTltCoins(coins, 5, 1);
assert.deepEqual(
  selected.map((coin) => coin.value),
  [5, 1],
);
assert.throws(() => selectTltCoins(coins, 0, 1), /amount must be > 0/);
assert.throws(() => selectTltCoins([{ tx_id: "01".repeat(32), index: 0, value: 5 }], 5, 1));

const leaves = [new Uint8Array(32).fill(1), new Uint8Array(32).fill(2), new Uint8Array(32).fill(3)];
const root = tltTxMerkleRoot(leaves);
const proof = proveTltTxMerkle(leaves, 2);
assert.ok(proof);
assert.deepEqual(proof.siblings[0], leaves[2]);
assert.equal(verifyTltTxMerkle(root, proof), true);
proof.txId[0] ^= 0xff;
assert.equal(verifyTltTxMerkle(root, proof), false);

const inclusion = proveTltTxMerkle(leaves, 1);
assert.ok(inclusion);
assert.equal(
  verifyTridentLight({
    txMerkleRoot: root,
    inclusion,
    headerTxRoot: root,
    powWorkMet: true,
    ovlSigned: 2n,
    ovlActive: 3n,
    drcSigned: 2n,
    drcActive: 3n,
  }),
  "Finalized",
);
assert.equal(
  verifyTridentLight({
    txMerkleRoot: root,
    inclusion,
    powWorkMet: true,
    ovlSigned: 2n,
    ovlActive: 3n,
    drcSigned: 1n,
    drcActive: 3n,
  }),
  "AwaitingDrcQuorum",
);
assert.throws(() =>
  verifyTridentLight({
    txMerkleRoot: root,
    inclusion,
    headerTxRoot: new Uint8Array(32).fill(9),
    powWorkMet: true,
    ovlSigned: 3n,
    ovlActive: 3n,
    drcSigned: 3n,
    drcActive: 3n,
  }),
);

console.log("tlt bitcoin light-client vectors ok");
