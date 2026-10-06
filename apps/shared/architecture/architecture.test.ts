/**
 * Architecture slice for community sections 48–59.
 * Run from apps/shared:
 * `node --experimental-strip-types --import ./light-client/register-ts-ext.mjs architecture/architecture.test.ts`
 */
import assert from "node:assert/strict";

import * as secp from "@noble/secp256k1";

import { proposalBadge, recordVote } from "../assembly/index.ts";
import { ACADEMY_CERTIFICATE } from "../academy/index.ts";
import { BOUNTY_DIRECTORY, markBountyPaid } from "../bounties/index.ts";
import { presentCached } from "../community/cache.ts";
import { broadcastDrcPay } from "../community/pay.ts";
import { encodeAgoraQr, parseAgoraQr } from "../community/qr.ts";
import { applyReputationEvent, emptyScores } from "../community/reputation.ts";
import {
  antiSybilDecision,
  assertCommunitySpendAllowed,
  createRateLimiter,
  exportCommunitySession,
  issueSession,
} from "../community/session.ts";
import type { Grant } from "../community/types.ts";
import {
  SERVICE_NAMES,
  SYBIL_RESISTANCE,
  inProcessServices,
  placementFor,
} from "../core/index.ts";
import { TX_BUILDERS, buildOvlContractCall } from "../core/txBuilders.ts";
import { claimAttendance } from "../events/index.ts";
import {
  assetTxLink,
  developerTrail,
  grantTrail,
  merchantTrail,
  minerTrail,
  plannedMarketLink,
} from "../explorer/index.ts";
import { likePost } from "../forum/index.ts";
import { GRANT_DISBURSEMENT, acceptGrantMilestone } from "../grants/index.ts";
import { joinGuild } from "../guilds/index.ts";
import { proofLabel, proveTltTxMerkle, tltTxMerkleRoot, verifyTltTxMerkle } from "../lightclient/index.ts";
import { ovlExecutionPreimage } from "../ovl/index.ts";
import { openMerchantSeal, sealMerchantPay } from "../merchants/index.ts";
import { missionSettlement, transitionMission } from "../missions/index.ts";
import { CLIENT_MODULES } from "../moduleMap.ts";
import {
  defaultNotificationPrefs,
  notificationBody,
} from "../notifications/index.ts";
import {
  acceptClientAttestation,
  createNonceLedger,
  reputationEvidence,
  signClientAttestation,
} from "../passport/index.ts";
import { assertPublicPayload, authorize } from "../security/authz.ts";
import { clipboardDecision } from "../security/clipboard.ts";
import { plannedHardwareSigner } from "../security/desktop.ts";
import { SCREENSHOT_PRIVACY, createPinRecord, pinMatches } from "../security/mobile.ts";
import { phishingWarnings } from "../security/phishing.ts";
import { previewNativeTransfer, signAfterPreview } from "../security/preview.ts";
import {
  exportDeviceSeed,
  lockDeviceSpendSession,
  openDeviceSpendSession,
} from "../security/session.ts";
import {
  architecturePanelModel,
  beginnerHasConsensusJargon,
  beginnerNavText,
} from "../settings/index.ts";
import { TREASURY_SPEND } from "../treasury/index.ts";
import { generateMnemonic, openVault, sealVault } from "../wallet/index.ts";

const evidence = "11".repeat(32);

function draftGrant(): Grant {
  return {
    id: "local-draft",
    title: "draft",
    asset: "DRC",
    beneficiary: "aa".repeat(20),
    total: "10",
    milestones: [{ id: "m1", title: "first", evidenceHash: null, accepted: false }],
    source: "community submitted",
    disbursesFunds: false,
  };
}

async function main(): Promise<void> {
  assert.equal(CLIENT_MODULES.length, 23);
  assert.ok(CLIENT_MODULES.includes("community"));
  assert.ok(CLIENT_MODULES.includes("lightclient"));

  assert.equal(placementFor("mnemonic"), "USER_PRIVATE");
  assert.equal(placementFor("tlt.merkle"), "BLOCKCHAIN");
  assert.equal(placementFor("node-reported-balance"), "INDEXER");
  assert.equal(placementFor("notification-pref"), "NOTIFICATIONS");
  assert.notEqual(placementFor("community-profile"), "BLOCKCHAIN");

  const services = inProcessServices();
  for (const name of SERVICE_NAMES) {
    const read = await services[name].read();
    assert.equal(read.fabricated, false);
    assert.equal(read.records.length, 0);
    assert.equal(read.status, "PLANNED");
    const write = await services[name].mutate();
    assert.equal(write.accepted, false);
    assert.equal(write.fabricated, false);
    assert.ok(services[name].trust.mustNotClaim.length > 0);
  }

  assert.equal(typeof TX_BUILDERS.drcPayment.build, "function");
  assert.equal(TX_BUILDERS.drcPayment.status, "present");
  assert.equal(TX_BUILDERS.ovlTransfer.status, "present");
  assert.equal(TX_BUILDERS.tltTransfer.status, "present");
  assert.equal(TX_BUILDERS.ovlContractCall.status, "PLANNED");
  assert.throws(() => buildOvlContractCall(), /PLANNED/);
  assert.throws(
    () =>
      ovlExecutionPreimage({
        from: new Uint8Array(20),
        to: new Uint8Array(20).fill(1),
        value: 1n,
        gasLimit: 21_000n,
        maxFeePerGas: 1n,
        nonce: 0n,
        data: new Uint8Array([1]),
        chainId: "agora-dev",
        genesis: new Uint8Array(32),
      }),
    /not active/,
  );

  const left = new Uint8Array(32).fill(1);
  const right = new Uint8Array(32).fill(2);
  const root = tltTxMerkleRoot([left, right]);
  const proof = proveTltTxMerkle([left, right], 0);
  assert.ok(proof);
  assert.equal(verifyTltTxMerkle(root, proof), true);
  proof.siblings[0] = new Uint8Array(32).fill(9);
  assert.equal(verifyTltTxMerkle(root, proof), false);
  assert.equal(proofLabel("tlt.merkle"), "verified-locally");
  assert.equal(proofLabel("drc.object"), "node-reported");
  assert.equal(proofLabel("ovl.contract"), "PLANNED");

  const phrase = generateMnemonic();
  const sealed = await sealVault(phrase, "correct-horse");
  assert.equal(await openVault(sealed, "correct-horse"), phrase);
  const spend = openDeviceSpendSession(phrase, 1_000, 5_000);
  assert.equal(JSON.stringify(spend).includes(phrase.split(" ")[0]!), false);
  assert.throws(() => exportDeviceSeed(spend, 1_100, false), /explicit reveal/);
  lockDeviceSpendSession(spend);
  assert.throws(() => exportDeviceSeed(spend, 1_200, true), /cannot export seed/);
  const expired = openDeviceSpendSession(phrase, 0, 1_000);
  assert.throws(() => exportDeviceSeed(expired, 1_000, true), /cannot export seed/);

  const communitySession = issueSession("agora1example", 0, "ab".repeat(16));
  assert.throws(
    () => exportCommunitySession({ ...communitySession, mnemonic: phrase }),
    /cannot export seed/,
  );
  assert.equal(authorize("spend", "export-seed").allowed, false);
  assert.equal(authorize("watch-only", "sign").allowed, false);
  assert.equal(authorize("spend", "sign").allowed, true);
  assert.throws(() => assertCommunitySpendAllowed("watch-only"), /watch-only/);
  assert.throws(() => assertPublicPayload({ mnemonic: phrase }), /must not leave/);

  const secret = secp.utils.randomPrivateKey();
  const ledger = createNonceLedger();
  const signed = await signClientAttestation(secret, {
    issuer: "issuer-1",
    subject: "subject-1",
    category: "Code",
    evidenceHash: evidence,
    nonce: "7",
  });
  assert.equal(signed.chainInclusion, "PLANNED");
  const accepted = await acceptClientAttestation(ledger, signed);
  const evidenceOnly = reputationEvidence([accepted]);
  assert.equal(evidenceOnly.score, "PLANNED");
  assert.equal(evidenceOnly.likesIgnored, true);
  await assert.rejects(() => acceptClientAttestation(ledger, signed), /replay/);
  await assert.rejects(
    () => acceptClientAttestation(createNonceLedger(), { ...signed, subject: "other" }),
    /rejected/,
  );

  const scores = emptyScores();
  const liked = applyReputationEvent(scores, {
    kind: "like",
    category: "Community",
    evidenceId: "post-1",
  });
  assert.equal(liked.applied, false);
  assert.equal(liked.scores.Community, 0);
  const contributed = applyReputationEvent(scores, {
    kind: "mission_complete",
    category: "Community",
    evidenceId: "mission-1",
  });
  assert.equal(contributed.applied, true);
  assert.equal(likePost().appliedToReputation, false);

  assert.throws(() => transitionMission("AVAILABLE", "COMPLETED"), /illegal/);
  assert.equal(missionSettlement("AVAILABLE").payment, null);
  assert.equal(missionSettlement("COMPLETED").onChain, "PLANNED");

  const granted = acceptGrantMilestone(draftGrant(), "m1", evidence);
  assert.equal(granted.disbursesFunds, false);
  assert.equal(granted.milestones[0]?.accepted, true);
  assert.equal(GRANT_DISBURSEMENT, "PLANNED");
  assert.throws(() => markBountyPaid(), /PLANNED/);
  assert.equal(BOUNTY_DIRECTORY.records.length, 0);
  assert.equal(BOUNTY_DIRECTORY.fabricated, false);

  const badge = proposalBadge({
    id: "p1",
    area: "community",
    title: "poll",
    summary: "advisory",
    binding: "on-chain",
    chainCommitment: null,
    source: "community submitted",
  });
  assert.equal(badge.badge, "ADVISORY");
  assert.throws(() => recordVote(), /PLANNED/);

  const payBody = encodeAgoraQr({
    version: 1,
    kind: "merchant-pay",
    fields: { to: "agora1exampledestination", amount: "5", merchant: "label-only" },
  });
  const sealedQr = await sealMerchantPay(payBody, secret);
  const opened = await openMerchantSeal(sealedQr);
  assert.equal(opened.confirmed, false);
  assert.equal(opened.paymentSubmitted, false);
  assert.equal(opened.merchantProfile, "PLANNED");
  assert.equal(opened.amount, "5");
  await assert.rejects(
    () => openMerchantSeal(sealedQr.replace("amount=5", "amount=9")),
    /does not match/,
  );
  const retarget = parseAgoraQr("agora:1:merchant-pay?to=agora1exampledestination&amount=5&asset=DRC");
  assert.equal(retarget.ok, false);
  const fractional = parseAgoraQr("agora:1:drc-payment?to=agora1exampledestination&amount=1.5");
  assert.equal(fractional.ok, false);
  const duplicate = parseAgoraQr(
    "agora:1:drc-payment?to=agora1exampledestination&amount=5&to=agora1other",
  );
  assert.equal(duplicate.ok, false);

  const drc = parseAgoraQr(
    encodeAgoraQr({
      version: 1,
      kind: "drc-payment",
      fields: { to: "agora1exampledestination", amount: "2" },
    }),
  );
  assert.equal(drc.ok, true);
  if (!drc.ok) throw new Error("payment QR");
  const receipt = broadcastDrcPay({ mode: "signing", preview: drc.preview, signed: true });
  assert.equal(receipt.confirmed, false);
  assert.equal(receipt.broadcast, "unavailable");

  const prefs = defaultNotificationPrefs();
  assert.equal(prefs.includeAmounts, false);
  assert.equal(JSON.stringify(prefs).includes("mnemonic"), false);
  const push = notificationBody("Mission", "Update ready, payout 3 DRC");
  assert.equal(push.body.includes("3"), false);
  assert.equal(presentCached(false, prefs, true).confirmed, false);

  const limiter = createRateLimiter(1, 1_000);
  assert.equal(limiter.allow("addr", 10), true);
  assert.equal(limiter.allow("addr", 20), false);
  const gate = antiSybilDecision({
    signatureValid: true,
    rateAllowed: true,
    reputation: 2,
    reputationThreshold: 1,
  });
  assert.equal(gate.pass, true);
  assert.equal(SYBIL_RESISTANCE, "PLANNED");

  const preview = previewNativeTransfer({
    asset: "DRC",
    to: "agora1exampledestination",
    amount: "4",
    fee: "1",
  });
  assert.equal(preview.signed, false);
  assert.equal(preview.submitted, false);
  let signed = false;
  await assert.rejects(
    signAfterPreview(preview, { confirmed: false }, async () => {
      signed = true;
    }),
    /not confirmed/,
  );
  assert.equal(signed, false);
  await signAfterPreview(preview, { confirmed: true, id: preview.id }, async () => {
    signed = true;
  });
  assert.equal(signed, true);

  const pin = createPinRecord("1234");
  assert.equal(pin.holdsSeed, false);
  assert.equal(JSON.stringify(pin).includes("mnemonic"), false);
  assert.equal(pinMatches(pin, "1234"), true);
  assert.equal(pinMatches(pin, "9999"), false);
  assert.equal(SCREENSHOT_PRIVACY.android, "PLANNED");
  assert.equal(SCREENSHOT_PRIVACY.ios, "PLANNED");
  assert.equal(clipboardDecision("mnemonic", false).allow, false);
  assert.equal(clipboardDecision("address", false).allow, true);

  const warnings = phishingWarnings("http://127.0.0.1:8545/rpc");
  assert.ok(warnings.some((line) => line.includes("Plain HTTP")));
  assert.ok(warnings.some((line) => line.includes("Localhost")));
  assert.ok(phishingWarnings("https://xn--agora-example.test/rpc").length > 0);
  await assert.rejects(plannedHardwareSigner().signPreview(), /PLANNED/);

  assert.equal(beginnerHasConsensusJargon(beginnerNavText()), false);
  const beginner = architecturePanelModel("beginner", "http://127.0.0.1:8545/rpc");
  assert.equal(beginner.seedOnScreen, false);
  assert.equal(
    beginner.surfaces.some((item) => item.label.includes("UTXO") || item.label.includes("RPC")),
    false,
  );
  const advanced = architecturePanelModel("advanced", "https://node.example/rpc");
  assert.ok(advanced.surfaces.some((item) => item.id === "contract-call" && item.status === "PLANNED"));
  assert.ok(advanced.surfaces.some((item) => item.id === "utxo"));

  assert.equal(assetTxLink("DRC", "ab".repeat(32)).status, "present");
  assert.equal(assetTxLink("TLT", "not-a-tx").status, "PLANNED");
  assert.equal(plannedMarketLink("dex").status, "PLANNED");
  assert.equal(plannedMarketLink("nft").href, null);
  const trail = grantTrail({ grantId: "g1" });
  assert.equal(trail[1]?.status, "PLANNED");
  assert.equal(merchantTrail({})[0]?.status, "PLANNED");
  assert.equal(developerTrail({}).find((item) => item.label === "projects")?.status, "PLANNED");
  assert.equal(minerTrail({ statsId: "miner-1" })[0]?.status, "PLANNED");
  assert.equal(ACADEMY_CERTIFICATE.status, "PLANNED");
  assert.equal(TREASURY_SPEND, "PLANNED");
  assert.equal(joinGuild("Merchant").member, false);
  assert.throws(() => claimAttendance(), /PLANNED/);

  console.log("architecture.test.ts ok");
}

main().catch((err: unknown) => {
  console.error(err);
  process.exit(1);
});
