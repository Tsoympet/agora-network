/**
 * HTTP host for community infrastructure. Light clients call it. They do not import it.
 *
 *   node --experimental-strip-types infrastructure/community-services/server.ts
 */
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { randomBytes } from "node:crypto";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { communityFixtures, demoPrivateProfile } from "../../apps/shared/community/fixtures.ts";
import {
  antiSybilDecision,
  createRateLimiter,
  createSessionChallenge,
  exportCommunitySession,
  issueSession,
  verifySessionSignature,
  type CommunitySession,
  type SessionChallenge,
} from "../../apps/shared/community/session.ts";
import { createEventService } from "../event-service/index.ts";
import { FORUM_TRUST, createForumServer } from "../forum-server/index.ts";
import { GRANT_ADMIN_TRUST, createGrantAdmin } from "../grant-admin/index.ts";
import { INDEXER_TRUST, createIndexer } from "../indexer/index.ts";
import { MODERATION_TRUST, createModerationPipeline } from "../moderation/index.ts";
import { NOTIFICATION_TRUST, createNotificationDispatcher } from "../notification-dispatch/index.ts";
import { SEARCH_TRUST, createSearchIndex } from "../search/index.ts";
import { openCommunityStore } from "./store.ts";

const port = Number(process.env.AGORA_COMMUNITY_PORT ?? 8787);

const SECRET_KEYS = new Set([
  "mnemonic",
  "seed",
  "privatekey",
  "secretkey",
  "password",
  "xprv",
  "vault",
]);

function defaultStorePath(): string {
  if (process.env.AGORA_COMMUNITY_STORE) return process.env.AGORA_COMMUNITY_STORE;
  if (process.argv[1]?.endsWith("server.ts")) {
    return fileURLToPath(new URL("./data/community-store.json", import.meta.url));
  }
  return join(tmpdir(), `agora-community-${process.pid}-${Date.now()}-${randomBytes(4).toString("hex")}.json`);
}

function rejectSecretKeys(value: unknown): void {
  if (!value || typeof value !== "object") return;
  for (const key of Object.keys(value as object)) {
    if (SECRET_KEYS.has(key.toLowerCase())) {
      throw new Error("community session cannot carry a seed");
    }
  }
}

const COMMUNITY_TRUST =
  "Operator-hosted community directory. Rows are community submitted unless a full node committed them. This host does not serve treasury balances.";

function envelope(service: string, trust: string, data: unknown, error?: string) {
  return {
    plane: "infrastructure" as const,
    service,
    trust,
    chainProof: false as const,
    data,
    chainData: null,
    ...(error ? { error } : {}),
  };
}

function send(res: ServerResponse, status: number, body: unknown) {
  res.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "cache-control": "no-store",
  });
  res.end(JSON.stringify(body));
}

async function readBody(req: IncomingMessage): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) chunks.push(Buffer.from(chunk));
  return Buffer.concat(chunks).toString("utf8");
}

function bearer(req: IncomingMessage, sessions: Map<string, CommunitySession>): CommunitySession | null {
  const header = req.headers.authorization;
  if (!header?.startsWith("Bearer ")) return null;
  const token = header.slice("Bearer ".length).trim();
  const session = sessions.get(token);
  if (!session || session.expiresAt < Date.now()) return null;
  return exportCommunitySession(session);
}

function communityRoutes(bundle: ReturnType<typeof communityFixtures>): Record<string, unknown> {
  return {
    "/passport": bundle.passports,
    "/reputation": bundle.passports.map((row) => ({
      address: row.address,
      reputations: row.reputations,
      overall: row.overall,
      source: row.source,
    })),
    "/bounties": bundle.bounties,
    "/academy": bundle.academy,
    "/merchants": bundle.merchants,
    "/guilds": bundle.guilds,
    "/proposals": bundle.proposals,
    "/contributions": bundle.contributions,
    "/hubs": bundle.hubs,
    "/developers": bundle.developers,
    "/ecosystem": bundle.ecosystem,
  };
}

function hexToBytes(hex: string): Uint8Array {
  const s = hex.startsWith("0x") ? hex.slice(2) : hex;
  if (s.length % 2 !== 0) throw new Error("invalid hex");
  const out = new Uint8Array(s.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = Number.parseInt(s.slice(i * 2, i * 2 + 2), 16);
  return out;
}

export function createCommunityInfrastructureServer(options?: { storePath?: string }) {
  const store = openCommunityStore(options?.storePath ?? defaultStorePath());
  const bundle = store.data.bundle;
  const forum = createForumServer(bundle.forum, {
    onChange: () => store.replaceForum(forum.list()),
  });
  const grants = createGrantAdmin({
    grants: bundle.grants,
    missions: bundle.missions,
    reviews: store.data.reviews,
    onChange: () => store.replaceMissions(grants.missions(), grants.reviews()),
  });
  const events = createEventService(bundle.events);
  const search = createSearchIndex(
    bundle.docs.map((doc) => ({ id: doc.id, title: doc.title, body: doc.body })),
  );
  const moderation = createModerationPipeline();
  const notifications = createNotificationDispatcher();
  const indexer = createIndexer({ upstreamRpc: process.env.AGORA_INDEXER_UPSTREAM_RPC ?? null });
  const challenges = new Map<string, SessionChallenge>();
  const sessions = new Map<string, CommunitySession>();
  const limiter = createRateLimiter(30, 60_000);
  const routes = communityRoutes(bundle);

  return createServer(async (req, res) => {
    const url = new URL(req.url ?? "/", "http://127.0.0.1");
    const path = url.pathname;
    if (!limiter.allow(req.socket.remoteAddress ?? "local", Date.now())) {
      send(res, 429, envelope("community", COMMUNITY_TRUST, null, "rate limit"));
      return;
    }
    try {
      if (req.method === "GET" && path === "/v1/indexer/status") {
        send(res, 200, envelope("indexer", INDEXER_TRUST, indexer.status()));
        return;
      }
      if (req.method === "POST" && path === "/v1/indexer/rpc") {
        try {
          const body = JSON.parse((await readBody(req)) || "{}") as unknown;
          const data = await indexer.proxy(body);
          send(res, 200, envelope("indexer", INDEXER_TRUST, data));
        } catch (err) {
          const status = (err as { status?: number }).status ?? 502;
          const message = err instanceof Error ? err.message : "indexer unavailable";
          send(res, status, envelope("indexer", INDEXER_TRUST, null, message));
        }
        return;
      }
      if (req.method === "GET" && path === "/forum") {
        send(res, 200, envelope("forum", FORUM_TRUST, forum.list()));
        return;
      }
      if (req.method === "POST" && path === "/forum/replies") {
        const body = JSON.parse((await readBody(req)) || "{}") as {
          postId?: string;
          body?: string;
          authorAddress?: string;
          authorUsername?: string;
        };
        rejectSecretKeys(body);
        if (!body.postId || !body.body || !body.authorAddress) {
          send(res, 400, envelope("forum", FORUM_TRUST, null, "postId, body, and authorAddress required"));
          return;
        }
        const reply = forum.reply({
          postId: body.postId,
          reply: {
            id: `reply-${randomBytes(8).toString("hex")}`,
            authorAddress: body.authorAddress,
            authorUsername: body.authorUsername?.trim() || body.authorAddress,
            body: body.body,
            source: "community submitted",
          },
        });
        send(res, 200, envelope("forum", FORUM_TRUST, reply));
        return;
      }
      if (req.method === "GET" && path === "/grants") {
        send(res, 200, envelope("grant-admin", GRANT_ADMIN_TRUST, grants.grants()));
        return;
      }
      if (req.method === "GET" && path === "/missions") {
        send(res, 200, envelope("grant-admin", GRANT_ADMIN_TRUST, grants.missions()));
        return;
      }
      if (req.method === "POST" && path === "/missions/advance") {
        const body = JSON.parse((await readBody(req)) || "{}") as { id?: string; to?: string };
        if (!body.id || !body.to) {
          send(res, 400, envelope("grant-admin", GRANT_ADMIN_TRUST, null, "id and to required"));
          return;
        }
        const data = grants.advance(body.id, body.to);
        send(res, 200, envelope("grant-admin", GRANT_ADMIN_TRUST, data));
        return;
      }
      if (req.method === "POST" && path === "/missions/review") {
        const body = JSON.parse((await readBody(req)) || "{}") as { missionId?: string; note?: string };
        if (!body.missionId || !body.note) {
          send(res, 400, envelope("grant-admin", GRANT_ADMIN_TRUST, null, "missionId and note required"));
          return;
        }
        send(res, 200, envelope("grant-admin", GRANT_ADMIN_TRUST, grants.review(body.missionId, body.note)));
        return;
      }
      if (req.method === "GET" && path === "/events") {
        send(res, 200, envelope("event-service", events.trust, events.list()));
        return;
      }
      if (req.method === "GET" && path === "/search") {
        send(res, 200, envelope("search", SEARCH_TRUST, search.query(url.searchParams.get("q") ?? "")));
        return;
      }
      if (req.method === "POST" && path === "/moderation/reports") {
        const body = JSON.parse((await readBody(req)) || "{}") as { postId?: string; reason?: string };
        const data = moderation.report({ postId: body.postId ?? "", reason: body.reason ?? "" });
        send(res, 200, envelope("moderation", MODERATION_TRUST, data));
        return;
      }
      if (req.method === "POST" && path === "/notifications/register") {
        const body = JSON.parse((await readBody(req)) || "{}") as { token?: string };
        const data = notifications.register(body.token ?? "");
        send(res, 200, envelope("notification-dispatch", NOTIFICATION_TRUST, data));
        return;
      }
      if (req.method === "POST" && path === "/notifications/dispatch") {
        const body = JSON.parse((await readBody(req)) || "{}") as { title?: string; body?: string };
        const data = notifications.dispatch({ title: body.title ?? "", body: body.body ?? "" });
        send(res, 200, envelope("notification-dispatch", NOTIFICATION_TRUST, data));
        return;
      }
      if (req.method === "POST" && path === "/session/challenge") {
        const body = JSON.parse((await readBody(req)) || "{}") as { address?: string };
        if (!body.address) {
          send(res, 400, envelope("community", COMMUNITY_TRUST, null, "address required"));
          return;
        }
        const challenge = createSessionChallenge(body.address, Date.now(), randomBytes(16).toString("hex"));
        challenges.set(challenge.nonce, challenge);
        send(res, 200, envelope("community", COMMUNITY_TRUST, challenge));
        return;
      }
      if (req.method === "POST" && path === "/session") {
        const body = JSON.parse((await readBody(req)) || "{}") as {
          challenge?: SessionChallenge;
          publicKey?: string;
          signature?: string;
          reputation?: number;
        };
        rejectSecretKeys(body);
        if (!body.challenge || !body.publicKey || !body.signature) {
          send(res, 400, envelope("community", COMMUNITY_TRUST, null, "challenge, publicKey, and signature required"));
          return;
        }
        const known = challenges.get(body.challenge.nonce);
        if (!known) {
          send(res, 400, envelope("community", COMMUNITY_TRUST, null, "unknown challenge"));
          return;
        }
        const signatureValid = await verifySessionSignature(
          known,
          hexToBytes(body.publicKey),
          body.signature,
          Date.now(),
        );
        const decision = antiSybilDecision({
          signatureValid,
          rateAllowed: true,
          reputation: body.reputation ?? 0,
          reputationThreshold: 0,
        });
        if (!decision.pass) {
          send(res, 401, envelope("community", COMMUNITY_TRUST, null, decision.reasons.join(", ")));
          return;
        }
        const session = issueSession(known.address, Date.now(), randomBytes(32).toString("hex"));
        sessions.set(session.token, session);
        send(res, 200, envelope("community", COMMUNITY_TRUST, exportCommunitySession(session)));
        return;
      }
      if (req.method === "GET" && path === "/passport/private") {
        const session = bearer(req, sessions);
        if (!session) {
          send(res, 401, envelope("community", COMMUNITY_TRUST, null, "community session required"));
          return;
        }
        send(res, 200, envelope("community", COMMUNITY_TRUST, {
          ...demoPrivateProfile,
          address: session.address,
          storage: "authenticated-service",
          consensus: false,
        }));
        return;
      }
      if (req.method === "GET" && path === "/treasury") {
        send(res, 404, envelope("community", COMMUNITY_TRUST, null, "treasury balances are on-chain and are not served here"));
        return;
      }
      if (req.method === "GET" && path in routes) {
        send(res, 200, envelope("community", COMMUNITY_TRUST, routes[path]));
        return;
      }
      send(res, 404, envelope("community", COMMUNITY_TRUST, null, "not found"));
    } catch (err) {
      send(res, 400, envelope("community", COMMUNITY_TRUST, null, err instanceof Error ? err.message : "bad request"));
    }
  });
}

if (process.argv[1]?.endsWith("server.ts")) {
  createCommunityInfrastructureServer().listen(port, "127.0.0.1", () => {
    console.log(`agora community infrastructure listening on ${port}`);
  });
}
