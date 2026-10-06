import { overallScore } from "./reputation.ts";
import type {
  CommunityBundle,
  PrivatePassportProfile,
  PublicPassport,
  ReputationScores,
} from "./types.ts";

const scores: ReputationScores = {
  Builder: 2,
  Community: 1,
  Merchant: 0,
  Security: 1,
  Mining: 0,
  Governance: 1,
  Education: 0,
};

export const DEMO_ADDRESS = "agora1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqdemo";

export const demoPassport: PublicPassport = {
  schemaVersion: 1,
  address: DEMO_ADDRESS,
  username: "harbor-builder",
  avatarHash: null,
  roles: ["builder"],
  reputations: scores,
  overall: overallScore(scores),
  badges: [
    {
      id: "badge-mission-1",
      name: "First mission",
      category: "Community",
      nonTransferable: true,
      evidenceId: "mission-docs-review",
      source: "community submitted",
    },
  ],
  contributionCounts: { missions: 1, votes: 1, certs: 0 },
  source: "community submitted",
};

export const demoPrivateProfile: PrivatePassportProfile = {
  schemaVersion: 1,
  address: DEMO_ADDRESS,
  email: "harbor@example.invalid",
  language: "en",
  storage: "local",
  consensus: false,
};

export function communityFixtures(): CommunityBundle {
  return {
    schemaVersion: 1,
    maturity: "Experimental",
    passports: [demoPassport],
    missions: [
      {
        id: "mission-docs-review",
        title: "Review the light-client threat model",
        summary: "Read the published light-client doc and file corrections.",
        state: "COMPLETED",
        category: "Education",
        rewardNote: "Reputation event only. No automatic token payout.",
        source: "community submitted",
      },
      {
        id: "mission-hub-charter",
        title: "Draft a hub charter hash",
        summary: "Propose a charter for a regional hub. No GPS is collected.",
        state: "AVAILABLE",
        category: "Community",
        rewardNote: "Completion can record a mission event after review.",
        source: "community submitted",
      },
    ],
    academy: {
      schemaVersion: 1,
      courses: [
        {
          id: "course-assets",
          track: "Protocol",
          title: "Three native assets",
          lessons: [
            { id: "lesson-drc", title: "DRC moves value", source: "community submitted" },
            { id: "lesson-ovl", title: "OVL builds value", source: "community submitted" },
            { id: "lesson-tlt", title: "TLT secures value", source: "community submitted" },
          ],
          source: "community submitted",
        },
      ],
    },
    grants: [
      {
        id: "grant-docs",
        title: "Documentation grant",
        asset: "DRC",
        beneficiary: DEMO_ADDRESS,
        total: "0",
        milestones: [
          {
            id: "ms-1",
            title: "Publish the community doc",
            evidenceHash: null,
            accepted: false,
          },
        ],
        source: "community submitted",
        disbursesFunds: false,
      },
    ],
    bounties: [
      {
        id: "bounty-qr",
        title: "Fail-closed QR parser review",
        category: "Security",
        status: "open",
        source: "community submitted",
      },
    ],
    guilds: [
      { id: "guild-builder", kind: "Builder", charter: "Protocol and client builders", memberCount: 1, source: "community submitted" },
      { id: "guild-node", kind: "Node", charter: "Full-node operators", memberCount: 0, source: "community submitted" },
      { id: "guild-miner", kind: "Miner", charter: "TLT RandomX miners", memberCount: 0, source: "community submitted" },
      { id: "guild-merchant", kind: "Merchant", charter: "DRC payment recipients", memberCount: 1, source: "community submitted" },
      { id: "guild-security", kind: "Security", charter: "Reviewers", memberCount: 0, source: "community submitted" },
      { id: "guild-education", kind: "Education", charter: "Academy authors", memberCount: 0, source: "community submitted" },
      { id: "guild-community", kind: "Community", charter: "Hub coordinators", memberCount: 0, source: "community submitted" },
    ],
    merchants: [
      {
        id: "merchant-harbor",
        displayName: "Harbor Press",
        drcAddress: "agora1merchantharborpress000000000000000000000000000000",
        region: "Aegean",
        category: "print",
        invoiceMemo: "desk copy",
        source: "community submitted",
        holdsMerchantKeys: false,
      },
    ],
    events: [
      {
        id: "event-assembly-notes",
        title: "Advisory assembly notes",
        region: "Aegean",
        startsAt: "2026-10-20T18:00:00Z",
        hubId: "hub-aegean",
        source: "community submitted",
      },
    ],
    hubs: [
      {
        id: "hub-aegean",
        name: "Aegean Hub",
        region: "Aegean",
        kind: "geographic",
        charterHash: null,
        source: "community submitted",
      },
      {
        id: "hub-security",
        name: "Security Guild Hub",
        region: "remote",
        kind: "specialist",
        charterHash: null,
        source: "community submitted",
      },
    ],
    forum: [
      {
        id: "post-welcome",
        category: "Announcements",
        authorAddress: DEMO_ADDRESS,
        authorUsername: "harbor-builder",
        title: "Community slice is experimental",
        body: "Posts are community submitted. Likes do not move reputation.",
        replies: [],
        reportCount: 0,
        source: "community submitted",
      },
    ],
    proposals: [
      {
        id: "prop-advisory-copy",
        area: "community",
        title: "Prefer plain-language payment labels",
        summary: "Ask clients to keep DRC / OVL / TLT role labels.",
        binding: "advisory",
        chainCommitment: null,
        source: "community submitted",
      },
      {
        id: "prop-false-chain",
        area: "ovl",
        title: "Uncommitted parameter note",
        summary: "Marked on-chain with no commitment id, so clients keep it advisory.",
        binding: "on-chain",
        chainCommitment: null,
        source: "community submitted",
      },
    ],
    treasuries: [],
    contributions: [
      {
        id: "contrib-1",
        kind: "mission_complete",
        title: "Review the light-client threat model",
        at: "2026-10-01T00:00:00Z",
        source: "community submitted",
      },
    ],
    developers: [
      {
        id: "dev-harbor",
        name: "harbor-builder",
        address: DEMO_ADDRESS,
        focus: "light clients",
        source: "community submitted",
      },
    ],
    docs: [
      {
        id: "doc-roles",
        title: "Asset roles",
        body: "DRC moves value. OVL builds value. TLT secures value. Community connects people.",
        source: "community submitted",
      },
    ],
    ecosystem: {
      schemaVersion: 1,
      passportCount: 1,
      missionCount: 2,
      grantCount: 1,
      merchantCount: 1,
      eventCount: 1,
      source: "community submitted",
    },
  };
}

export function merchantForAddress(
  merchants: CommunityBundle["merchants"],
  address: string,
): CommunityBundle["merchants"][number] | null {
  const needle = address.trim().toLowerCase();
  return merchants.find((merchant) => merchant.drcAddress.toLowerCase() === needle) ?? null;
}

export function reportForumPost(
  posts: CommunityBundle["forum"],
  postId: string,
): CommunityBundle["forum"] {
  return posts.map((post) =>
    post.id === postId ? { ...post, reportCount: post.reportCount + 1 } : post,
  );
}

export function searchHubs(
  hubs: CommunityBundle["hubs"],
  query: { region?: string; name?: string },
): CommunityBundle["hubs"] {
  const region = query.region?.trim().toLowerCase() ?? "";
  const name = query.name?.trim().toLowerCase() ?? "";
  return hubs.filter((hub) => {
    if (region && !hub.region.toLowerCase().includes(region)) return false;
    if (name && !hub.name.toLowerCase().includes(name)) return false;
    return true;
  });
}
