import { localCommunityFromJson, type LocalCommunity } from "./localCommunities.ts";
import type { AuthorityBadge } from "./proposalTransparency.ts";
import {
  PUBLIC_ANALYTICS_KEYS,
  type PublicAnalyticsAggregates,
  type PublicAnalyticsKey,
} from "./publicAnalytics.ts";

export type HubInput = {
  id: string;
  name: string;
  region: string;
  kind: string;
};

export type CommunityProposalInput = {
  id: string;
  title: string;
  summary: string;
  binding: "on-chain" | "advisory";
  chainCommitment: string | null;
  eligibility?: string;
};

export type EcosystemCounts = {
  missionCount: number;
  grantCount: number;
  merchantCount: number;
  eventCount: number;
};

export type ServiceProposalView = {
  id: string;
  badge: AuthorityBadge;
  note: string;
  lines: Array<{ label: string; value: string }>;
};

export function hubToLocalCommunity(hub: HubInput): LocalCommunity {
  const specialization = hub.kind.trim() || "community";
  return localCommunityFromJson({
    id: slug(hub.id),
    name: hub.name,
    region: hub.region,
    specializations: [specialization],
  });
}

/** A community-service proposal without a commitment id stays ADVISORY. */
export function serviceProposalView(input: CommunityProposalInput): ServiceProposalView {
  const committed = input.binding === "on-chain" && Boolean(input.chainCommitment?.trim());
  const badge: AuthorityBadge = committed ? "ON-CHAIN" : "ADVISORY";
  const note = committed
    ? "Commitment id is indexed from a full node. This light client does not re-check validator signatures."
    : input.binding === "on-chain"
      ? "Marked on-chain without a commitment id, so the badge stays ADVISORY."
      : "Advisory record. It does not execute protocol changes.";
  return {
    id: input.id,
    badge,
    note,
    lines: [
      { label: "Creator", value: "not included in the community proposal record" },
      { label: "Why", value: input.summary.trim() || "not stated" },
      { label: "What changes", value: input.title.trim() || "not stated" },
      { label: "Expected cost", value: "not stated" },
      { label: "Treasury impact", value: "not stated" },
      { label: "Voting period", value: "not published" },
      { label: "Eligibility", value: input.eligibility?.trim() || "not stated on this record" },
      { label: "Current votes", value: "not published" },
      { label: "Final result", value: badge === "ADVISORY" ? "advisory_recorded" : "not published" },
      { label: "Implementation", value: "not published" },
      {
        label: "Commit / commitment",
        value: input.chainCommitment?.trim() || "none",
      },
      { label: "Releases", value: "none" },
    ],
  };
}

export function annotatePublicAnalytics(ecosystem: EcosystemCounts | null): Array<{
  key: PublicAnalyticsKey;
  counted: boolean;
  value: number | null;
}> {
  const counted = new Map<PublicAnalyticsKey, number>();
  if (ecosystem) {
    counted.set("missions", ecosystem.missionCount);
    counted.set("grants", ecosystem.grantCount);
    counted.set("active_merchants", ecosystem.merchantCount);
    counted.set("events", ecosystem.eventCount);
  }
  return PUBLIC_ANALYTICS_KEYS.map((key) =>
    counted.has(key)
      ? { key, counted: true, value: counted.get(key) ?? 0 }
      : { key, counted: false, value: null },
  );
}

export function countedAggregate(ecosystem: EcosystemCounts | null): PublicAnalyticsAggregates {
  const aggregates = {} as PublicAnalyticsAggregates;
  for (const row of annotatePublicAnalytics(ecosystem)) {
    aggregates[row.key] = row.value ?? 0;
  }
  return aggregates;
}

function slug(id: string): string {
  const cleaned = id.trim().toLowerCase().replace(/[^a-z0-9-]/g, "-").replace(/-+/g, "-");
  const trimmed = cleaned.replace(/^-|-$/g, "");
  if (!/^[a-z0-9]/.test(trimmed)) return `hub-${trimmed || "record"}`.slice(0, 64);
  return trimmed.slice(0, 64);
}
