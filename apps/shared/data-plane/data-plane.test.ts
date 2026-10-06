/**
 * Data-plane labels and HTTP facades.
 * Run from the repo root:
 *   node --experimental-strip-types apps/shared/data-plane/data-plane.test.ts
 */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { createInfrastructureFacades, InfrastructureUnconfigured } from "./facades.ts";
import { DATA_PLANE_FEATURES } from "./features.ts";
import { DATA_PLANES, type DataPlane } from "./planes.ts";

const REQUIRED: Record<DataPlane, string[]> = {
  device: [
    "wallet",
    "light-verification",
    "passport-ui",
    "community-ui",
    "drc-payments",
    "qr",
    "missions-ui",
    "academy-ui",
    "governance-ui",
    "merchant-tools",
    "notifications-ui",
    "explorer-ui",
    "guilds-ui",
  ],
  infrastructure: [
    "indexer",
    "forum-backend",
    "grant-mission-admin",
    "event-service",
    "notification-dispatch",
    "search",
    "moderation",
  ],
  "on-chain": [
    "tlt-balances-txs",
    "ovl-balances-txs",
    "drc-balances-txs",
    "governance-txs",
    "treasury-txs",
    "credential-attestations",
  ],
};

const FORBIDDEN_ON_CHAIN = ["private-profile", "forum-posts", "mission-reviews", "academy-progress"];

const ids = DATA_PLANE_FEATURES.map((row) => row.id);
assert.equal(new Set(ids).size, ids.length, "feature ids are unique");

for (const row of DATA_PLANE_FEATURES) {
  assert.ok(DATA_PLANES.includes(row.plane), row.id);
  if (row.onChainForbidden) {
    assert.notEqual(row.plane, "on-chain", row.id);
    assert.equal(row.onChain, null, row.id);
  }
  if (row.plane === "infrastructure") {
    assert.ok(row.trust, `${row.id} documents trust`);
  }
}

for (const plane of DATA_PLANES) {
  for (const id of REQUIRED[plane]) {
    const row = DATA_PLANE_FEATURES.find((feature) => feature.id === id);
    assert.ok(row, id);
    assert.equal(row?.plane, plane, id);
  }
}

for (const id of FORBIDDEN_ON_CHAIN) {
  const row = DATA_PLANE_FEATURES.find((feature) => feature.id === id);
  assert.ok(row, id);
  assert.equal(row?.onChainForbidden, true, id);
}

const doc = readFileSync(new URL("../../../docs/core/AGORA_DATA_PLANE_SPLIT.md", import.meta.url), "utf8");
for (const row of DATA_PLANE_FEATURES) {
  assert.ok(doc.includes(`\`${row.id}\``), `doc missing ${row.id}`);
}
assert.match(doc, /chainProof/);
assert.match(doc, /does not invent/);

const calls: string[] = [];
const facades = createInfrastructureFacades({
  baseUrl: "http://infra.test",
  fetchImpl: async (input, init) => {
    calls.push(`${init?.method ?? "GET"} ${input}`);
    const url = new URL(input);
    const service = url.pathname.startsWith("/forum")
      ? "forum"
      : url.pathname.startsWith("/v1/indexer")
        ? "indexer"
        : "search";
    return {
      ok: true,
      status: 200,
      json: async () => ({
        plane: "infrastructure",
        service,
        trust: "operator",
        chainProof: false,
        data: service === "indexer" ? { upstream: false, chainData: null } : [],
      }),
    };
  },
});

assert.equal("dispatch" in facades.notifications, false);
const posts = await facades.forum.list();
assert.deepEqual(posts, []);
assert.equal(calls[0], "GET http://infra.test/forum");
const status = await facades.indexer.status();
assert.equal(status.chainData, null);
await facades.search.query("hub");
assert.equal(calls[2], "GET http://infra.test/search?q=hub");

const offline = createInfrastructureFacades({ baseUrl: "" });
await assert.rejects(() => offline.forum.list(), InfrastructureUnconfigured);

const lying = createInfrastructureFacades({
  baseUrl: "http://infra.test",
  fetchImpl: async () => ({
    ok: true,
    status: 200,
    json: async () => ({ plane: "on-chain", service: "forum", chainProof: true, data: [{ balance: "100" }] }),
  }),
});
await assert.rejects(() => lying.forum.list(), /must not claim a chain proof/);

console.log("data-plane.test.ts ok");
