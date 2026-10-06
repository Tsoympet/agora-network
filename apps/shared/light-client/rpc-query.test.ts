/**
 * Shared light-client query wrappers.
 * Run: `node --experimental-strip-types light-client/rpc-query.test.ts`
 */
import assert from "node:assert/strict";

import { createLightClient } from "./rpc.ts";

const calls: Array<{ method: string; params: unknown }> = [];
const originalFetch = globalThis.fetch;
globalThis.fetch = (async (_url: unknown, init?: RequestInit) => {
  const body = JSON.parse(String(init?.body)) as {
    method: string;
    params: unknown;
  };
  calls.push({ method: body.method, params: body.params });
  return {
    ok: true,
    json: async () => ({ result: { ok: true, method: body.method } }),
  } as Response;
}) as typeof fetch;

try {
  const client = createLightClient({ rpcUrl: "http://127.0.0.1/rpc" });
  await client.getAccountBalances("agoradev1qqqq");
  await client.getTltCovenant("aa".repeat(32));
  await client.getDrcOffer("bb".repeat(32));
  await client.getDrcAccountOffers({ account: "agoradev1qqqq", limit: 8 });
  await client.getDrcBookOffers({
    book: { taker_gets: { native: true }, taker_pays: { native: false } },
    cursor: "c1",
  });

  assert.deepEqual(
    calls.map((call) => call.method),
    [
      "agora_getAccountBalances",
      "agora_getTltCovenant",
      "agora_getDrcOffer",
      "agora_getDrcAccountOffers",
      "agora_getDrcBookOffers",
    ],
  );
  assert.deepEqual(calls[0].params, { address: "agoradev1qqqq" });
  assert.deepEqual(calls[1].params, { tx_id: "aa".repeat(32) });
  assert.deepEqual(calls[2].params, { offer_id: "bb".repeat(32) });
  assert.deepEqual(calls[3].params, {
    account: "agoradev1qqqq",
    limit: 8,
  });
  assert.equal(
    (calls[4].params as { cursor?: string }).cursor,
    "c1",
  );
  console.log("light-client query wrappers ok");
} finally {
  globalThis.fetch = originalFetch;
}
