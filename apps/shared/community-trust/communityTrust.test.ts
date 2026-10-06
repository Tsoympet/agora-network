/**
 * Run from apps/shared:
 * node --experimental-strip-types --import ./light-client/register-ts-ext.mjs community-trust/communityTrust.test.ts
 */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  annotatePublicAnalytics,
  authorityBadge,
  communityTrustView,
  defaultTreasuryInputs,
  evaluateReward,
  exampleProposals,
  exportPortableIdentity,
  localCommunityFromJson,
  parsePortableIdentity,
  parsePublicAnalytics,
  requestsDeviceLocation,
  rewardControl,
  rewardProgramFromInputs,
  scaffoldPublicAnalytics,
  searchLocalCommunities,
  seedLocalCommunities,
  serviceProposalView,
  TRUST_ASSUMPTION_IDS,
  TRUST_ASSUMPTIONS,
  validateProposalTransparency,
} from "./index.ts";

const here = dirname(fileURLToPath(import.meta.url));
const seeds = seedLocalCommunities();

assert.equal(requestsDeviceLocation(), false);
assert.equal(searchLocalCommunities(seeds, "   ").length, 4);
assert.equal(searchLocalCommunities(seeds, "Greece")[0]?.city, "Athens");
assert.equal(searchLocalCommunities(seeds, "europe")[0]?.id, "europe");
assert.equal(searchLocalCommunities(seeds, "Asia")[0]?.region, "Asia");
assert.equal(searchLocalCommunities(seeds, "Americas")[0]?.region, "Americas");
assert.equal(searchLocalCommunities(seeds, "37.9838,23.7275").length, 0);
assert.equal(searchLocalCommunities(seeds, "athens asia").length, 0);

const custom = localCommunityFromJson({
  id: "example-city",
  name: "Example",
  country: "Not A Listed Country",
  languages: ["en"],
});
assert.equal(custom.country, "Not A Listed Country");
assert.throws(
  () => localCommunityFromJson({ id: "gps", name: "GPS", country: "Greece", latitude: 37.9 }),
  /latitude/,
);

const [onChain, advisory] = exampleProposals();
assert.equal(onChain?.badge, "ON-CHAIN");
assert.equal(authorityBadge("on_chain"), "ON-CHAIN");
assert.equal(advisory?.badge, "ADVISORY");
assert.throws(() => {
  if (!advisory) throw new Error("missing advisory");
  validateProposalTransparency({ ...advisory, final_result: "passed" });
}, /advisory/);
assert.throws(() => {
  if (!advisory) throw new Error("missing advisory");
  validateProposalTransparency({ ...advisory, implementation_status: "shipped" });
}, /commit or release/);

const program = rewardProgramFromInputs(defaultTreasuryInputs());
const emission = evaluateReward(program.find((row) => row.label === "TLT from emission")!);
assert.equal(emission.ui_blocked, true);
assert.equal(emission.changes_tlt_emission, false);
assert.equal(emission.supply_delta_base_units, "0");
assert.equal(rewardControl(emission).disabled, true);
assert.equal(rewardControl(emission).submits_transaction, false);

const funded = evaluateReward({
  kind: "tlt",
  label: "TLT from existing treasury",
  source: { type: "existing_treasury", available_base_units: "9", amount_base_units: "4" },
});
assert.equal(funded.payable, true);
assert.equal(funded.supply_delta_base_units, "0");
assert.equal(rewardControl(funded).submits_transaction, false);
assert.match(funded.funding_note, /already-issued TLT/);

const hugeTreasury = rewardProgramFromInputs({
  ...defaultTreasuryInputs(),
  tlt_available: "1000000",
});
const stillBlocked = evaluateReward(hugeTreasury.find((row) => row.source.type === "emission")!);
assert.equal(stillBlocked.ui_blocked, true);

const analytics = scaffoldPublicAnalytics();
assert.equal(analytics.private_user_analytics, false);
assert.equal(analytics.aggregates.active_users, 0);
parsePublicAnalytics(analytics.aggregates);
assert.throws(
  () => parsePublicAnalytics({ ...analytics.aggregates, email: "a@example.com" }),
  /email/,
);
assert.throws(
  () => parsePublicAnalytics({ ...analytics.aggregates, active_users: ["agora1secret"] }),
  /active_users/,
);

const json = exportPortableIdentity({
  subject_address: "agora1example",
  language: "en",
  region_query: "Greece",
  attestations: [
    {
      category: "code",
      issuer_label: "Athens hub",
      evidence_hash_hex: "ab".repeat(32),
    },
  ],
});
assert.equal(json.includes('"mnemonic"'), false);
assert.equal(json.includes('"private_key"'), false);
assert.equal(json.includes('"xprv"'), false);
const bundle = parsePortableIdentity(json);
assert.equal(bundle.contains_private_keys, false);
assert.equal(bundle.service_trust.length, 10);
assert.equal(bundle.service_trust[0]?.service, "headers");
assert.throws(
  () =>
    exportPortableIdentity({
      region_query: "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu",
    }),
  /key material/,
);
assert.throws(() => parsePortableIdentity(json.replace('"format"', '"mnemonic": "no", "format"')), /mnemonic/);

assert.deepEqual(
  TRUST_ASSUMPTIONS.map((row) => row.id),
  [...TRUST_ASSUMPTION_IDS],
);
for (const row of TRUST_ASSUMPTIONS) {
  assert.ok(row.assumption.length > 40);
  assert.ok(row.checks.length > 10);
  assert.equal("hidden" in row, false);
}

const security = readFileSync(
  join(here, "../../../docs/core/LIGHT_CLIENT_SECURITY_MODEL.md"),
  "utf8",
);
for (const id of TRUST_ASSUMPTION_IDS) {
  assert.ok(security.includes(`\`${id}\``), `security model missing ${id}`);
}
for (const row of TRUST_ASSUMPTIONS) {
  assert.ok(security.includes(row.assumption), `security model missing assumption for ${row.id}`);
}

const aegean = searchLocalCommunities(
  [
    ...seeds,
    {
      id: "hub-aegean",
      name: "Aegean Hub",
      region: "Aegean",
      languages: [],
      interests: [],
      specializations: ["geographic"],
    },
  ],
  "aegean",
);
assert.equal(aegean[0]?.name, "Aegean Hub");
assert.equal(aegean[0]?.country, undefined);

const falseChain = serviceProposalView({
  id: "prop-false-chain",
  title: "Uncommitted parameter note",
  summary: "Claims an on-chain binding without a commitment.",
  binding: "on-chain",
  chainCommitment: null,
});
assert.equal(falseChain.badge, "ADVISORY");
const indexed = serviceProposalView({
  id: "prop-indexed-chain",
  title: "Indexed commitment example",
  summary: "Shown as on-chain because a commitment id is indexed.",
  binding: "on-chain",
  chainCommitment: "indexed-commitment-example",
});
assert.equal(indexed.badge, "ON-CHAIN");
assert.equal(authorityBadge("advisory"), "ADVISORY");
const rows = annotatePublicAnalytics({
  missionCount: 2,
  grantCount: 1,
  merchantCount: 4,
  eventCount: 3,
});
assert.equal(rows.find((row) => row.key === "missions")?.value, 2);
assert.equal(rows.find((row) => row.key === "active_users")?.counted, false);

const view = communityTrustView({
  query: "Greece",
  treasury: defaultTreasuryInputs(),
  subjectAddress: "agora1example",
  language: "en",
  interestFilters: ["governance"],
});
assert.equal(view.communities.length, 1);
assert.equal(view.assumptions.length, 10);
assert.equal(view.analytics.private_user_analytics, false);
assert.ok("json" in view.identity);
const panel = readFileSync(join(here, "../../desktop/src/components/CommunityTrustPanel.tsx"), "utf8");
const phone = readFileSync(join(here, "../../mobile/CommunityTrustPanel.tsx"), "utf8");
for (const source of [panel, phone]) {
  assert.equal(source.includes("geolocation"), false);
  assert.equal(source.includes("expo-location"), false);
  assert.ok(source.includes("TRUST_ASSUMPTIONS") || source.includes("assumptions.map"));
  assert.ok(source.includes("control.disabled"));
}

console.log("communityTrust.test.ts ok");
