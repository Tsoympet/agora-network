/**
 * Static index of community readiness notes.
 * The list is repository metadata. It does not query a node.
 */

export const COMMUNITY_DOC_BRANCH = "cursor/agora-community-docs-cdcf";

export const COMMUNITY_DOC_MATURITIES = [
  "Scaffold",
  "Experimental",
  "Single-node prototype",
] as const;

export type CommunityDocMaturity = (typeof COMMUNITY_DOC_MATURITIES)[number];

export type CommunityDoc = {
  id: string;
  title: string;
  path: string;
  maturity: CommunityDocMaturity;
  summary: string;
};

export const COMMUNITY_DOCS: readonly CommunityDoc[] = [
  {
    id: "architecture",
    title: "Community architecture",
    path: "docs/core/AGORA_COMMUNITY_ARCHITECTURE.md",
    maturity: "Scaffold",
    summary:
      "Source labels, trust boundary, and the planned MY AGORA information architecture.",
  },
  {
    id: "passport",
    title: "Passport",
    path: "docs/core/AGORA_PASSPORT.md",
    maturity: "Scaffold",
    summary: "Signed, non-transferable attestations. Registry writes are still a library path.",
  },
  {
    id: "reputation",
    title: "Reputation",
    path: "docs/core/AGORA_REPUTATION.md",
    maturity: "Scaffold",
    summary: "Contribution scores are specified as a separate plane and are not stored yet.",
  },
  {
    id: "assembly",
    title: "Assembly",
    path: "docs/core/AGORA_ASSEMBLY.md",
    maturity: "Experimental",
    summary: "Civic engine and forum RPC. Ballots are administrative node state.",
  },
  {
    id: "missions",
    title: "Missions",
    path: "docs/core/AGORA_MISSIONS.md",
    maturity: "Scaffold",
    summary: "Lifecycle records exist. Completion does not pay a treasury.",
  },
  {
    id: "academy",
    title: "Academy",
    path: "docs/core/AGORA_ACADEMY.md",
    maturity: "Scaffold",
    summary: "Education program named in the community notes. No course state.",
  },
  {
    id: "grants",
    title: "Grants",
    path: "docs/core/AGORA_GRANTS.md",
    maturity: "Scaffold",
    summary: "Milestone records and a conflict-of-interest gate. No disbursement.",
  },
  {
    id: "bounties",
    title: "Bounties",
    path: "docs/core/AGORA_BOUNTIES.md",
    maturity: "Scaffold",
    summary: "Named next to grants. No bounty type on this commit.",
  },
  {
    id: "guilds",
    title: "Guilds",
    path: "docs/core/AGORA_GUILDS.md",
    maturity: "Scaffold",
    summary: "Builder and node guilds are program names. No charter state.",
  },
  {
    id: "merchants",
    title: "Merchant network",
    path: "docs/core/AGORA_MERCHANT_NETWORK.md",
    maturity: "Scaffold",
    summary: "DRC payment primitives exist. A merchant directory does not.",
  },
  {
    id: "events",
    title: "Events",
    path: "docs/core/AGORA_EVENTS.md",
    maturity: "Scaffold",
    summary: "Calendar kinds are listed. No event record.",
  },
  {
    id: "hubs",
    title: "Hubs",
    path: "docs/core/AGORA_HUBS.md",
    maturity: "Scaffold",
    summary: "Hub records and the passport issuer index. Accreditation is not a block lane.",
  },
  {
    id: "treasury",
    title: "Treasury",
    path: "docs/core/AGORA_TREASURY.md",
    maturity: "Scaffold",
    summary: "Three asset-matched treasuries. Civic spend proposals do not debit them.",
  },
  {
    id: "light-client",
    title: "Light client",
    path: "docs/core/AGORA_LIGHT_CLIENT.md",
    maturity: "Single-node prototype",
    summary: "Index for the PC and phone verifier. Detail stays in agora-light-client.md.",
  },
  {
    id: "light-security",
    title: "Light client security",
    path: "docs/core/AGORA_LIGHT_CLIENT_SECURITY_MODEL.md",
    maturity: "Experimental",
    summary: "What the device recomputes, and what still trusts one full node.",
  },
  {
    id: "mobile",
    title: "Phone architecture",
    path: "docs/core/AGORA_MOBILE_ARCHITECTURE.md",
    maturity: "Single-node prototype",
    summary: "Expo shell over the shared verifier. SecureStore holds the vault.",
  },
  {
    id: "pc",
    title: "PC architecture",
    path: "docs/core/AGORA_PC_ARCHITECTURE.md",
    maturity: "Single-node prototype",
    summary: "Desktop shell over the same verifier, plus the civic ballot panel.",
  },
  {
    id: "privacy",
    title: "Privacy model",
    path: "docs/core/AGORA_PRIVACY_MODEL.md",
    maturity: "Scaffold",
    summary: "Public attestations, device-held keys, and a private profile that is not built.",
  },
  {
    id: "api",
    title: "Community API",
    path: "docs/core/AGORA_COMMUNITY_API.md",
    maturity: "Scaffold",
    summary: "Future HTTP surface. Node JSON-RPC is not this API.",
  },
  {
    id: "phases",
    title: "Implementation phases",
    path: "docs/core/AGORA_COMMUNITY_PHASES.md",
    maturity: "Scaffold",
    summary: "C1–C23 with IMPLEMENTED, IN DEVELOPMENT, or PLANNED.",
  },
  {
    id: "definition-of-done",
    title: "Definition of done",
    path: "docs/core/AGORA_COMMUNITY_DEFINITION_OF_DONE.md",
    maturity: "Scaffold",
    summary: "Checklist. The community ecosystem is incomplete on this commit.",
  },
];

export function communityDocHref(path: string): string {
  return `https://github.com/Tsoympet/agora-network/blob/${COMMUNITY_DOC_BRANCH}/${path}`;
}
