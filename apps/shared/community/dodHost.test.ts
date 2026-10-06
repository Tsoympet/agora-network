/**
 * Infrastructure host: durable JSON store, vault-signed sessions, forum replies.
 * Run from apps/shared:
 *   node --experimental-strip-types --import ./light-client/register-ts-ext.mjs community/dodHost.test.ts
 */
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import * as secp from "@noble/secp256k1";

import { createCommunityInfrastructureServer } from "../../../infrastructure/community-services/server.ts";
import { COMMUNITY_STORE_TRUST } from "../../../infrastructure/community-services/store.ts";
import { createCommunityClient } from "./client.ts";
import { createSessionChallenge, signSessionChallenge } from "./session.ts";

const root = fileURLToPath(new URL("../../..", import.meta.url));
const dod = readFileSync(join(root, "docs/core/AGORA_COMMUNITY_DEFINITION_OF_DONE.md"), "utf8");
assert.equal(dod.includes("COMMUNITY ECOSYSTEM — INCOMPLETE"), true);
assert.equal(dod.includes("COMMUNITY ECOSYSTEM — COMPLETE"), false);
assert.match(dod, /- \[ \]/);
const phases = readFileSync(join(root, "docs/core/AGORA_COMMUNITY_PHASES.md"), "utf8");
for (let id = 1; id <= 23; id += 1) {
  assert.match(phases, new RegExp(`\\| C${id} \\|`));
}

const dir = mkdtempSync(join(tmpdir(), "agora-community-dod-"));
const storePath = join(dir, "community-store.json");

function listen(store = storePath) {
  const server = createCommunityInfrastructureServer({ storePath: store });
  return new Promise<{ server: ReturnType<typeof createCommunityInfrastructureServer>; url: string }>((resolve) => {
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") throw new Error("no port");
      resolve({ server, url: `http://127.0.0.1:${address.port}` });
    });
  });
}

const first = await listen();
try {
  const client = createCommunityClient({ baseUrl: first.url });
  const posts = await client.forum();
  const postId = posts.data?.[0]?.id;
  assert.ok(postId);
  const reply = await client.replyToForum({
    postId,
    body: "shipped through the infrastructure host",
    authorAddress: "agora1author",
    authorUsername: "harbor",
  });
  assert.equal(reply.online, true);
  assert.equal(reply.data?.source, "community submitted");
  assert.equal(reply.confirmed, false);

  const secret = secp.utils.randomPrivateKey();
  const publicKey = secp.getPublicKey(secret, true);
  const challenge = await client.requestSessionChallenge("agora1session");
  const signature = await signSessionChallenge(secret, challenge);
  const session = await client.openSession({
    challenge,
    publicKey: Buffer.from(publicKey).toString("hex"),
    signature,
  });
  assert.equal(session.address, "agora1session");
  assert.equal(session.token.includes("mnemonic"), false);
  const profile = await client.privateProfile(session.token);
  assert.equal(profile.consensus, false);

  const mnemonicBody = await fetch(`${first.url}/session`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      challenge: createSessionChallenge("agora1session", Date.now(), "n"),
      publicKey: "ab",
      signature: "cd",
      mnemonic: "abandon abandon",
    }),
  });
  assert.equal(mnemonicBody.status, 400);

  const offline = createCommunityClient({ baseUrl: null });
  const localReply = await offline.replyToForum({
    postId: "post",
    body: "local",
    authorAddress: "agora1author",
  });
  assert.equal(localReply.data, null);
  assert.match(localReply.label, /did not start a forum server/);
} finally {
  await new Promise<void>((resolve, reject) => first.server.close((err) => (err ? reject(err) : resolve())));
}

const stored = JSON.parse(readFileSync(storePath, "utf8")) as {
  trust: string;
  bundle: { forum: { replies: { body: string }[] }[] };
};
assert.equal(stored.trust, COMMUNITY_STORE_TRUST);
assert.equal(
  stored.bundle.forum.some((post) => post.replies.some((reply) => reply.body.includes("infrastructure host"))),
  true,
);

const second = await listen();
try {
  const again = createCommunityClient({ baseUrl: second.url });
  const posts = await again.forum();
  assert.equal(
    (posts.data ?? []).some((post) => post.replies.some((reply) => reply.body.includes("infrastructure host"))),
    true,
  );
} finally {
  await new Promise<void>((resolve, reject) => second.server.close((err) => (err ? reject(err) : resolve())));
}

console.log("dodHost.test.ts ok");
