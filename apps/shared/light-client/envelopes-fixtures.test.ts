/**
 * Run: node --experimental-strip-types --import ./register-ts-ext.mjs envelopes-fixtures.test.ts
 */
import assert from "node:assert/strict";

import { fixturePreimages } from "./envelopes.ts";

const fixtures = fixturePreimages();
for (const [name, hex] of Object.entries(fixtures)) {
  assert.match(hex, /^[0-9a-f]+$/, `${name} preimage hex`);
  assert.ok(hex.length >= 64, `${name} preimage length`);
}
assert.ok(Object.keys(fixtures).length >= 8);

console.log("envelopes-fixtures.test.ts ok");
