/**
 * Run: node --experimental-strip-types light-client/featureMatrix.test.ts
 */
import assert from "node:assert/strict";

import {
  LIGHT_FEATURE_MATRIX,
  matrixWithRpcAvailability,
} from "./featureMatrix.ts";

assert.ok(LIGHT_FEATURE_MATRIX.length >= 20);
const unavailable = LIGHT_FEATURE_MATRIX.find((row) => row.id === "tlt.covenant");
assert.equal(unavailable?.status, "unavailable");

const merged = matrixWithRpcAvailability(LIGHT_FEATURE_MATRIX, new Set(["agora_getUtxos"]));
const utxo = merged.find((row) => row.id === "tlt.utxo_list");
assert.equal(utxo?.status, "node-reported");
const missing = merged.find((row) => row.id === "drc.escrow");
assert.equal(missing?.status, "unavailable");

console.log("featureMatrix.test.ts ok");
