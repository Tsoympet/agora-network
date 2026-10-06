/**
 * Local community API. Same types as the light clients.
 * Sessions are wallet-signature gated. The process keeps no seeds.
 *
 *   node --experimental-strip-types apps/community-api/server.ts
 */
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { randomBytes } from "node:crypto";

import { communityFixtures, demoPrivateProfile } from "../shared/community/fixtures.ts";
import {
  antiSybilDecision,
  createRateLimiter,
  createSessionChallenge,
  exportCommunitySession,
  issueSession,
  verifySessionSignature,
  type CommunitySession,
  type SessionChallenge,
} from "../shared/community/session.ts";

const bundle = communityFixtures();
const challenges = new Map<string, SessionChallenge>();
const sessions = new Map<string, CommunitySession>();
const limiter = createRateLimiter(30, 60_000);
const port = Number(process.env.AGORA_COMMUNITY_PORT ?? 8787);

function send(res: ServerResponse, status: number, body: unknown) {
  const json = JSON.stringify(body);
  res.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "cache-control": "no-store",
  });
  res.end(json);
}

async function readBody(req: IncomingMessage): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) chunks.push(Buffer.from(chunk));
  return Buffer.concat(chunks).toString("utf8");
}

function bearer(req: IncomingMessage): CommunitySession | null {
  const header = req.headers.authorization;
  if (!header?.startsWith("Bearer ")) return null;
  const token = header.slice("Bearer ".length).trim();
  const session = sessions.get(token);
  if (!session || session.expiresAt < Date.now()) return null;
  return exportCommunitySession(session);
}

const routes: Record<string, unknown> = {
  "/passport": bundle.passports,
  "/reputation": bundle.passports.map((row) => ({
    address: row.address,
    reputations: row.reputations,
    overall: row.overall,
    source: row.source,
  })),
  "/missions": bundle.missions,
  "/grants": bundle.grants,
  "/bounties": bundle.bounties,
  "/academy": bundle.academy,
  "/events": bundle.events,
  "/merchants": bundle.merchants,
  "/guilds": bundle.guilds,
  "/proposals": bundle.proposals,
  "/treasury": bundle.treasuries,
  "/contributions": bundle.contributions,
  "/hubs": bundle.hubs,
  "/forum": bundle.forum,
  "/developers": bundle.developers,
  "/ecosystem": bundle.ecosystem,
};

const server = createServer(async (req, res) => {
  const url = new URL(req.url ?? "/", "http://127.0.0.1");
  const path = url.pathname;
  if (!limiter.allow(req.socket.remoteAddress ?? "local", Date.now())) {
    send(res, 429, { error: "rate limit" });
    return;
  }
  try {
    if (req.method === "POST" && path === "/session/challenge") {
      const body = JSON.parse((await readBody(req)) || "{}") as { address?: string };
      if (!body.address) {
        send(res, 400, { error: "address required" });
        return;
      }
      const challenge = createSessionChallenge(body.address, Date.now(), randomBytes(16).toString("hex"));
      challenges.set(challenge.nonce, challenge);
      send(res, 200, challenge);
      return;
    }
    if (req.method === "POST" && path === "/session") {
      const body = JSON.parse((await readBody(req)) || "{}") as {
        challenge?: SessionChallenge;
        publicKey?: string;
        signature?: string;
        reputation?: number;
      };
      if (!body.challenge || !body.publicKey || !body.signature) {
        send(res, 400, { error: "challenge, publicKey, and signature required" });
        return;
      }
      const known = challenges.get(body.challenge.nonce);
      if (!known) {
        send(res, 400, { error: "unknown challenge" });
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
        send(res, 401, { error: decision.reasons.join(", ") });
        return;
      }
      const session = issueSession(known.address, Date.now(), randomBytes(32).toString("hex"));
      sessions.set(session.token, session);
      send(res, 200, exportCommunitySession(session));
      return;
    }
    if (req.method === "GET" && path === "/passport/private") {
      const session = bearer(req);
      if (!session) {
        send(res, 401, { error: "community session required" });
        return;
      }
      send(res, 200, {
        ...demoPrivateProfile,
        address: session.address,
        storage: "authenticated-service",
        consensus: false,
      });
      return;
    }
    if (req.method === "GET" && path in routes) {
      send(res, 200, routes[path]);
      return;
    }
    send(res, 404, { error: "not found", maturity: bundle.maturity });
  } catch (err) {
    send(res, 400, { error: err instanceof Error ? err.message : "bad request" });
  }
});

function hexToBytes(hex: string): Uint8Array {
  const s = hex.startsWith("0x") ? hex.slice(2) : hex;
  if (s.length % 2 !== 0) throw new Error("invalid hex");
  const out = new Uint8Array(s.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = Number.parseInt(s.slice(i * 2, i * 2 + 2), 16);
  return out;
}

if (process.argv[1]?.endsWith("server.ts")) {
  server.listen(port, "127.0.0.1", () => {
    console.log(`agora community api listening on ${port}`);
  });
}

export { server };
