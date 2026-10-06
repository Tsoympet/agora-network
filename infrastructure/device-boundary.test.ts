/**
 * Device packages must not import infrastructure implementations.
 * Run from the repo root:
 *   node --experimental-strip-types infrastructure/device-boundary.test.ts
 */
import assert from "node:assert/strict";
import { createServer, type ServerResponse } from "node:http";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

import { createInfrastructureFacades } from "../apps/shared/data-plane/facades.ts";
import { DEVICE_FORBIDDEN_IMPLEMENTATIONS } from "../apps/shared/data-plane/planes.ts";
import { createCommunityInfrastructureServer } from "./community-services/server.ts";
import { createForumServer } from "./forum-server/index.ts";
import { createIndexer } from "./indexer/index.ts";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));

const DEVICE_ROOTS = [
  "apps/desktop",
  "apps/mobile",
  "apps/explorer",
  "apps/shared/light-client",
  "apps/shared/community",
  "apps/shared/community-trust",
  "apps/shared/data-plane",
  "apps/shared/brand",
];

function walk(dir: string, out: string[]) {
  for (const entry of readdirSync(dir)) {
    if (entry === "node_modules" || entry === "dist" || entry === "target") continue;
    const path = join(dir, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) walk(path, out);
    else if (/\.(ts|tsx|js|mjs|css)$/.test(entry) && !entry.endsWith(".test.ts")) out.push(path);
  }
}

function importSpecifiers(source: string): string[] {
  const specs: string[] = [];
  const pattern =
    /(?:import|export)\s+(?:type\s+)?(?:[^'"\n]*?\s+from\s+)?["']([^"']+)["']|import\(\s*["']([^"']+)["']\s*\)|require\(\s*["']([^"']+)["']\s*\)/g;
  for (const match of source.matchAll(pattern)) {
    const spec = match[1] || match[2] || match[3];
    if (spec) specs.push(spec);
  }
  return specs;
}

const files: string[] = [];
for (const root of DEVICE_ROOTS) walk(join(repoRoot, root), files);
assert.ok(files.length > 10, "device tree was scanned");

const offenders: string[] = [];
for (const file of files) {
  const source = readFileSync(file, "utf8");
  for (const spec of importSpecifiers(source)) {
    if (spec.startsWith("node:http") || spec.startsWith("node:https")) {
      offenders.push(`${relative(repoRoot, file)} imports ${spec}`);
    }
    for (const forbidden of DEVICE_FORBIDDEN_IMPLEMENTATIONS) {
      if (spec.includes(forbidden)) offenders.push(`${relative(repoRoot, file)} imports ${spec}`);
    }
  }
}
assert.deepEqual(offenders, []);

assert.equal(statExists(join(repoRoot, "apps/community-api")), false, "community API left apps/");
assert.equal(statExists(join(repoRoot, "infrastructure/indexer/index.ts")), true);
assert.equal(statExists(join(repoRoot, "infrastructure/forum-server/index.ts")), true);

const host = readFileSync(join(repoRoot, "infrastructure/community-services/server.ts"), "utf8");
assert.match(host, /infrastructure\/forum-server|forum-server\/index/);
assert.match(host, /indexer\/index/);
assert.doesNotMatch(host, /"balance":\s*"[1-9]/);

function statExists(path: string): boolean {
  try {
    statSync(path);
    return true;
  } catch {
    return false;
  }
}

function send(res: ServerResponse, status: number, service: string, trust: string, data: unknown, error?: string) {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(JSON.stringify({
    plane: "infrastructure",
    service,
    trust,
    chainProof: false,
    data,
    chainData: null,
    ...(error ? { error } : {}),
  }));
}

const forum = createForumServer([
  {
    id: "post-1",
    category: "Announcements",
    authorAddress: "agora1example",
    authorUsername: "harbor",
    title: "boundary",
    body: "posted through HTTP",
    replies: [],
    reportCount: 0,
    source: "community submitted",
  },
]);
const indexer = createIndexer();

const server = createServer(async (req, res) => {
  const url = new URL(req.url ?? "/", "http://127.0.0.1");
  if (req.method === "GET" && url.pathname === "/forum") {
    send(res, 200, "forum", forum.trust, forum.list());
    return;
  }
  if (req.method === "GET" && url.pathname === "/v1/indexer/status") {
    send(res, 200, "indexer", indexer.trust, indexer.status());
    return;
  }
  if (req.method === "POST" && url.pathname === "/v1/indexer/rpc") {
    try {
      await indexer.proxy({});
      send(res, 200, "indexer", indexer.trust, null);
    } catch (err) {
      const status = (err as { status?: number }).status ?? 502;
      send(res, status, "indexer", indexer.trust, null, err instanceof Error ? err.message : "unavailable");
    }
    return;
  }
  send(res, 404, "community", "unused", null, "not found");
});

await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", () => resolve()));
const address = server.address();
if (!address || typeof address === "string") throw new Error("no port");
const baseUrl = `http://127.0.0.1:${address.port}`;

try {
  const facades = createInfrastructureFacades({ baseUrl });
  const posts = await facades.forum.list();
  assert.equal(Array.isArray(posts), true);
  assert.equal((posts as { title: string }[])[0]?.title, "boundary");
  const status = await facades.indexer.status();
  assert.equal(status.upstream, false);
  assert.equal(status.chainData, null);
  await assert.rejects(() => fetch(`${baseUrl}/v1/indexer/rpc`, { method: "POST", body: "{}" }).then(async (res) => {
    const body = await res.json() as { chainData: unknown; data: unknown; error?: string };
    assert.equal(res.status, 503);
    assert.equal(body.chainData, null);
    assert.equal(body.data, null);
    assert.match(body.error ?? "", /no upstream/);
    throw new Error(body.error ?? "no upstream");
  }), /no upstream/);
} finally {
  await new Promise<void>((resolve, reject) => server.close((err) => (err ? reject(err) : resolve())));
}

const communityHost = createCommunityInfrastructureServer();
await new Promise<void>((resolve) => communityHost.listen(0, "127.0.0.1", () => resolve()));
const hostAddress = communityHost.address();
if (!hostAddress || typeof hostAddress === "string") throw new Error("no host port");
const hostUrl = `http://127.0.0.1:${hostAddress.port}`;
try {
  const live = createInfrastructureFacades({ baseUrl: hostUrl });
  const livePosts = await live.forum.list();
  assert.ok(Array.isArray(livePosts));
  assert.ok((livePosts as unknown[]).length > 0);
  const liveIndex = await live.indexer.status();
  assert.equal(liveIndex.upstream, false);
  assert.equal(liveIndex.chainData, null);
  const advanced = await live.grants.advance({ id: "mission-hub-charter", to: "ACCEPTED" });
  assert.equal(advanced.recorded, true);
  const registered = await live.notifications.register("device-token");
  assert.equal(registered.registered, true);
  const treasury = await fetch(`${hostUrl}/treasury`);
  const treasuryBody = await treasury.json() as { data: unknown; chainProof: boolean; error?: string };
  assert.equal(treasury.status, 404);
  assert.equal(treasuryBody.data, null);
  assert.equal(treasuryBody.chainProof, false);
  assert.match(treasuryBody.error ?? "", /on-chain/);
  const proxy = await fetch(`${hostUrl}/v1/indexer/rpc`, { method: "POST", body: "{}" });
  const proxyBody = await proxy.json() as { data: unknown; chainData: unknown };
  assert.equal(proxy.status, 503);
  assert.equal(proxyBody.data, null);
  assert.equal(proxyBody.chainData, null);
  const leaked = await fetch(`${hostUrl}/notifications/dispatch`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ title: "Mission", body: "Reward 25 DRC" }),
  });
  assert.equal(leaked.status, 400);
} finally {
  await new Promise<void>((resolve, reject) => communityHost.close((err) => (err ? reject(err) : resolve())));
}

console.log("device-boundary.test.ts ok");
