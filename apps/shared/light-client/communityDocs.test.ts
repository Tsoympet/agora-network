/**
 * Run: node --experimental-strip-types light-client/communityDocs.test.ts
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { COMMUNITY_DOCS, COMMUNITY_DOC_MATURITIES } from "./communityDocs.ts";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../..");

const REQUIRED = [
  "docs/core/AGORA_COMMUNITY_ARCHITECTURE.md",
  "docs/core/AGORA_PASSPORT.md",
  "docs/core/AGORA_REPUTATION.md",
  "docs/core/AGORA_ASSEMBLY.md",
  "docs/core/AGORA_MISSIONS.md",
  "docs/core/AGORA_ACADEMY.md",
  "docs/core/AGORA_GRANTS.md",
  "docs/core/AGORA_BOUNTIES.md",
  "docs/core/AGORA_GUILDS.md",
  "docs/core/AGORA_MERCHANT_NETWORK.md",
  "docs/core/AGORA_EVENTS.md",
  "docs/core/AGORA_HUBS.md",
  "docs/core/AGORA_TREASURY.md",
  "docs/core/AGORA_LIGHT_CLIENT.md",
  "docs/core/AGORA_LIGHT_CLIENT_SECURITY_MODEL.md",
  "docs/core/AGORA_MOBILE_ARCHITECTURE.md",
  "docs/core/AGORA_PC_ARCHITECTURE.md",
  "docs/core/AGORA_PRIVACY_MODEL.md",
  "docs/core/AGORA_COMMUNITY_API.md",
  "docs/core/AGORA_COMMUNITY_PHASES.md",
  "docs/core/AGORA_COMMUNITY_DEFINITION_OF_DONE.md",
];

const HEADINGS = [
  "## Purpose",
  "## Data sources",
  "## Trust boundary",
  "## Maturity",
  "## Implemented",
  "## Planned",
];

const catalogPaths = new Set(COMMUNITY_DOCS.map((doc) => doc.path));
for (const rel of REQUIRED) {
  assert.equal(fs.existsSync(path.join(root, rel)), true, rel);
  assert.equal(catalogPaths.has(rel), true, `catalog missing ${rel}`);
}

for (const doc of COMMUNITY_DOCS) {
  assert.ok(
    (COMMUNITY_DOC_MATURITIES as readonly string[]).includes(doc.maturity),
    doc.id,
  );
  const text = fs.readFileSync(path.join(root, doc.path), "utf8");
  const maturity = text.match(
    /^\*\*Maturity:\*\* (Scaffold|Experimental|Single-node prototype)\s*$/m,
  );
  assert.ok(maturity, `${doc.path} maturity line`);
  assert.equal(maturity?.[1], doc.maturity, doc.path);
  for (const heading of HEADINGS) {
    assert.equal(text.includes(heading), true, `${doc.path} ${heading}`);
  }
  for (const label of ["blockchain", "community", "indexer", "private"]) {
    assert.equal(text.includes(`| ${label} |`), true, `${doc.path} ${label}`);
  }
  for (const banned of ["Mainnet ready", "Audited production", "Public testnet"]) {
    assert.equal(text.includes(banned), false, `${doc.path} ${banned}`);
  }
}

const phases = fs.readFileSync(
  path.join(root, "docs/core/AGORA_COMMUNITY_PHASES.md"),
  "utf8",
);
const rows = [
  ...phases.matchAll(
    /^\| (C\d+) \| (.+?) \| (IMPLEMENTED|IN DEVELOPMENT|PLANNED) \|$/gm,
  ),
];
assert.equal(rows.length, 23);
rows.forEach((row, index) => {
  assert.equal(row[1], `C${index + 1}`);
});

const dod = fs.readFileSync(
  path.join(root, "docs/core/AGORA_COMMUNITY_DEFINITION_OF_DONE.md"),
  "utf8",
);
assert.equal(dod.includes("COMMUNITY ECOSYSTEM — INCOMPLETE"), true);
assert.equal(dod.includes("COMMUNITY ECOSYSTEM — COMPLETE"), false);

const desktop = fs.readFileSync(
  path.join(root, "apps/desktop/src/App.tsx"),
  "utf8",
);
const phone = fs.readFileSync(path.join(root, "apps/mobile/App.tsx"), "utf8");
const explorer = fs.readFileSync(
  path.join(root, "apps/explorer/src/App.tsx"),
  "utf8",
);
assert.equal(desktop.includes("DocsAboutPanel"), true);
assert.equal(phone.includes("DocsAboutPanel"), true);
assert.equal(explorer.includes("CommunityDocsPanel"), true);

const light = fs.readFileSync(
  path.join(root, "docs/core/agora-light-client.md"),
  "utf8",
);
assert.equal(light.includes("AGORA_LIGHT_CLIENT.md"), true);
assert.equal(light.includes("AGORA_LIGHT_CLIENT_SECURITY_MODEL.md"), true);

console.log("communityDocs.test.ts ok");
