import {
  annotatePublicAnalytics,
  hubToLocalCommunity,
  serviceProposalView,
  type CommunityProposalInput,
  type EcosystemCounts,
  type HubInput,
  type ServiceProposalView,
} from "./fromCommunity.ts";
import {
  MANUAL_REGION_SEARCH_NOTICE,
  searchLocalCommunities,
  seedLocalCommunities,
  type LocalCommunity,
} from "./localCommunities.ts";
import { exportPortableIdentity, type PortableIdentityBundle } from "./portableIdentity.ts";
import { exampleProposals, type ProposalTransparencyView } from "./proposalTransparency.ts";
import {
  ANALYTICS_LABELS,
  scaffoldPublicAnalytics,
  type PublicAnalyticsKey,
  type PublicAnalyticsReport,
} from "./publicAnalytics.ts";
import {
  evaluateRewardProgram,
  rewardControl,
  rewardProgramFromInputs,
  type RewardControl,
  type RewardDecision,
  type TreasuryInputs,
} from "./rewards.ts";
import { TRUST_ASSUMPTIONS, type TrustAssumption } from "./trustAssumptions.ts";

export type RewardRow = RewardDecision & { control: RewardControl };

export type CommunityTrustView = {
  notice: string;
  communities: LocalCommunity[];
  proposals: ProposalTransparencyView[];
  rewards: RewardRow[];
  analytics: PublicAnalyticsReport;
  analyticsRows: ReturnType<typeof annotatePublicAnalytics>;
  analyticsLabels: typeof ANALYTICS_LABELS;
  serviceProposals: ServiceProposalView[];
  identity: { json: string; bundle: PortableIdentityBundle } | { error: string };
  assumptions: readonly TrustAssumption[];
};

export function communityTrustView(input: {
  query: string;
  treasury: TreasuryInputs;
  subjectAddress: string | null;
  language: string;
  interestFilters: string[];
  hubs?: readonly HubInput[];
  proposals?: readonly CommunityProposalInput[];
  ecosystem?: EcosystemCounts | null;
}): CommunityTrustView {
  const records = [
    ...seedLocalCommunities(),
    ...(input.hubs ?? []).map((hub) => hubToLocalCommunity(hub)),
  ];
  const communities = searchLocalCommunities(records, input.query);
  const rewards = evaluateRewardProgram(rewardProgramFromInputs(input.treasury)).map((decision) => ({
    ...decision,
    control: rewardControl(decision),
  }));
  let identity: CommunityTrustView["identity"];
  try {
    const json = exportPortableIdentity({
      subject_address: input.subjectAddress,
      language: input.language,
      region_query: input.query,
      interest_filters: input.interestFilters,
    });
    identity = { json, bundle: JSON.parse(json) as PortableIdentityBundle };
  } catch (err) {
    identity = { error: err instanceof Error ? err.message : "export refused" };
  }
  return {
    notice: MANUAL_REGION_SEARCH_NOTICE,
    communities,
    proposals: exampleProposals(),
    rewards,
    analytics: scaffoldPublicAnalytics(),
    analyticsRows: annotatePublicAnalytics(input.ecosystem ?? null),
    analyticsLabels: ANALYTICS_LABELS,
    serviceProposals: (input.proposals ?? []).map((proposal) => serviceProposalView(proposal)),
    identity,
    assumptions: TRUST_ASSUMPTIONS,
  };
}

export function analyticsEntries(
  report: PublicAnalyticsReport,
): Array<{ key: PublicAnalyticsKey; label: string; value: number }> {
  return (Object.keys(report.aggregates) as PublicAnalyticsKey[]).map((key) => ({
    key,
    label: ANALYTICS_LABELS[key],
    value: report.aggregates[key],
  }));
}
