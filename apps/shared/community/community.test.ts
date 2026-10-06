/**
 * Community SDK invariants.
 * Run: `node --experimental-strip-types community/community.test.ts`
 */
import assert from "node:assert/strict";

import { memoryCacheStorage, notificationBody, presentCached, saveCache, loadCache, emptyCache } from "./cache.ts";
import { createCommunityClient } from "./client.ts";
import { communityFixtures, demoPassport, demoPrivateProfile, searchHubs } from "./fixtures.ts";
import { proposalBadge, voteEligibility } from "./governance.ts";
import { transitionMission } from "./missions.ts";
import { advanceDrcPay, broadcastDrcPay, signDrcPayIntent } from "./pay.ts";
import { assertPaymentPreview, encodeAgoraQr, parseAgoraQr } from "./qr.ts";
import { applyReputationEvent, emptyScores, splitPassport, transferBadge } from "./reputation.ts";
import {
  antiSybilDecision,
  assertCommunitySpendAllowed,
  createRateLimiter,
  createSessionChallenge,
  exportCommunitySession,
  issueSession,
  signSessionChallenge,
  verifySessionSignature,
} from "./session.ts";
import * as secp from "@noble/secp256k1";

const secret = secp.utils.randomSecretKey();
const publicKey = secp.getPublicKey(secret, true);

const split = splitPassport({
  publicProfile: demoPassport,
  privateProfile: demoPrivateProfile,
});
assert.equal(split.privateProfile.consensus, false);
assert.equal(JSON.stringify(split.publicProfile).includes("harbor@example.invalid"), false);
assert.equal(split.publicProfile.badges[0]?.nonTransferable, true);
assert.throws(() => transferBadge(split.publicProfile.badges[0]!, "other"), /non-transferable/);

assert.throws(
  () => assertCommunitySpendAllowed("watch-only"),
  /watch-only wallets cannot sign/,
);
assert.doesNotThrow(() => assertCommunitySpendAllowed("signing"));

const payment = encodeAgoraQr({
  version: 1,
  kind: "drc-payment",
  fields: { to: "agora1dest", amount: "25" },
});
const parsed = parseAgoraQr(payment);
assert.equal(parsed.ok, true);
if (parsed.ok) {
  assert.equal(parsed.preview.destination, "agora1dest");
  assert.equal(parsed.preview.amount, "25");
  assertPaymentPreview(parsed.preview);
}
assert.equal(parseAgoraQr("agora1qqqq").ok, false);
assert.equal(parseAgoraQr("agora:1:drc-payment?to=a").ok, false);
assert.equal(parseAgoraQr("agora:1:drc-payment?to=a&to=b&amount=1").ok, false);
assert.equal(parseAgoraQr("agora:1:drc-payment?to=a&amount=1&asset=TLT").ok, false);
assert.equal(parseAgoraQr('{"agoraQr":1,"kind":"nope"}').ok, false);
assert.equal(parseAgoraQr("agora:9:address?addr=a").ok, false);
assert.equal(parseAgoraQr("").ok, false);

assert.equal(transitionMission("AVAILABLE", "ACCEPTED"), "ACCEPTED");
assert.equal(transitionMission("ACCEPTED", "IN_PROGRESS"), "IN_PROGRESS");
assert.equal(transitionMission("IN_PROGRESS", "SUBMITTED"), "SUBMITTED");
assert.equal(transitionMission("SUBMITTED", "COMPLETED"), "COMPLETED");
assert.throws(() => transitionMission("AVAILABLE", "COMPLETED"), /illegal mission/);
assert.throws(() => transitionMission("COMPLETED", "AVAILABLE"), /illegal mission/);

const advisory = proposalBadge({ binding: "advisory", chainCommitment: null });
assert.equal(advisory.badge, "ADVISORY");
const fakeChain = proposalBadge({ binding: "on-chain", chainCommitment: null });
assert.equal(fakeChain.badge, "ADVISORY");
const indexed = proposalBadge({ binding: "on-chain", chainCommitment: "abc" });
assert.equal(indexed.badge, "ON-CHAIN GOVERNANCE");
assert.equal(indexed.source, "indexed");

let scores = emptyScores();
const liked = applyReputationEvent(scores, {
  kind: "like",
  category: "Community",
  evidenceId: "post-1",
});
assert.equal(liked.applied, false);
assert.equal(liked.scores.Community, 0);
const done = applyReputationEvent(scores, {
  kind: "mission_complete",
  category: "Community",
  evidenceId: "mission-1",
});
assert.equal(done.applied, true);
assert.equal(done.scores.Community, 1);
scores = done.scores;

const session = issueSession(demoPassport.address, 1_000, "ab".repeat(16));
const exported = exportCommunitySession(session);
assert.equal("mnemonic" in exported, false);
assert.throws(
  () => exportCommunitySession({ ...session, mnemonic: "abandon abandon" } as typeof session),
  /session cannot export seed/,
);

const challenge = createSessionChallenge(demoPassport.address, 1_000, "nonce-1");
const signature = await signSessionChallenge(secret, challenge);
assert.equal(await verifySessionSignature(challenge, publicKey, signature, 1_000), true);
assert.equal(await verifySessionSignature(challenge, publicKey, signature, challenge.expiresAt + 1), false);

const limiter = createRateLimiter(2, 1_000);
assert.equal(limiter.allow("addr", 0), true);
assert.equal(limiter.allow("addr", 10), true);
assert.equal(limiter.allow("addr", 20), false);
const gate = antiSybilDecision({
  signatureValid: true,
  rateAllowed: false,
  reputation: 0,
  reputationThreshold: 1,
});
assert.equal(gate.pass, false);
assert.equal(gate.reasons.includes("rate limit"), true);

const preview = parsed.ok ? parsed.preview : null;
assert.ok(preview);
let step: ReturnType<typeof advanceDrcPay> = "scan";
for (const expected of ["inspect", "verify-merchant", "confirm", "sign", "broadcast", "receipt"] as const) {
  step = advanceDrcPay(step === "scan" ? "scan" : step);
  assert.equal(step, expected);
  if (step === "receipt") break;
}
assert.throws(() => signDrcPayIntent({
  mode: "watch-only",
  preview: preview!,
  merchant: { listed: false, displayName: null, source: "community submitted", holdsMerchantKeys: false },
  confirmed: true,
}), /watch-only/);
const signed = signDrcPayIntent({
  mode: "signing",
  preview: preview!,
  merchant: { listed: true, displayName: "Harbor Press", source: "community submitted", holdsMerchantKeys: false },
  confirmed: true,
});
assert.equal(signed.destination, "agora1dest");
const receipt = broadcastDrcPay({ mode: "signing", preview: preview!, signed: true });
assert.equal(receipt.confirmed, false);
assert.equal(receipt.broadcast, "unavailable");

const technical = voteEligibility("ovl", {
  reputations: { ...emptyScores(), Community: 4 },
  roles: [],
});
assert.equal(technical.eligible, false);
const builder = voteEligibility("ovl", {
  reputations: { ...emptyScores(), Builder: 1 },
  roles: [],
});
assert.equal(builder.eligible, true);

const note = notificationBody("Mission", "Review ready. Reward 25 DRC later");
assert.equal(note.body.includes("25"), false);

const storage = memoryCacheStorage();
const cached = emptyCache(5_000);
cached.passport = demoPassport;
cached.privateProfile = demoPrivateProfile;
cached.docs = communityFixtures().docs;
await saveCache(storage, cached);
const loaded = await loadCache(storage, 6_000);
assert.equal(loaded.passport?.username, "harbor-builder");
assert.equal(loaded.privateProfile?.consensus, false);
const offline = presentCached(false, loaded.passport, true);
assert.equal(offline.confirmed, false);
assert.match(offline.label, /not confirmed/);

const hubs = searchHubs(communityFixtures().hubs, { region: "aegean" });
assert.equal(hubs.length, 1);
assert.equal(searchHubs(communityFixtures().hubs, { region: "gps" }).length, 0);

const client = createCommunityClient({ baseUrl: null });
const missions = await client.missions();
assert.equal(missions.online, false);
assert.equal(missions.confirmed, false);
assert.ok((missions.data?.length ?? 0) > 0);
await assert.rejects(() => client.privateProfile(null), /community session/);
const profile = await client.privateProfile("ab".repeat(16));
assert.equal(profile.consensus, false);

console.log("agora-community: ok");
