import type {
  CommunityProposal,
  DataSource,
  GovernanceArea,
  GovernanceAreaId,
  PublicPassport,
  ReputationCategory,
} from "./types.ts";

export const GOVERNANCE_AREAS: readonly GovernanceArea[] = [
  {
    id: "technical",
    label: "Technical / Builder",
    eligibility: "Builder reputation or a seated technical role",
  },
  {
    id: "drc",
    label: "DRC / Payments",
    eligibility: "DRC account activity or Merchant/Community reputation",
  },
  {
    id: "ovl",
    label: "OVL / EVM",
    eligibility: "Builder reputation. Not open to every passport",
  },
  {
    id: "tlt",
    label: "TLT / Mining",
    eligibility: "Mining reputation or a disclosed miner role",
  },
  {
    id: "community",
    label: "Community",
    eligibility: "Any passport with Community reputation",
  },
  {
    id: "treasury",
    label: "Treasury",
    eligibility: "Governance reputation or a treasury role",
  },
  {
    id: "education",
    label: "Education",
    eligibility: "Education reputation or an academy certificate",
  },
  {
    id: "merchant",
    label: "Merchant",
    eligibility: "Merchant reputation or a listed merchant profile",
  },
];

const AREA_CATEGORY: Record<GovernanceAreaId, ReputationCategory | "role"> = {
  technical: "Builder",
  drc: "Community",
  ovl: "Builder",
  tlt: "Mining",
  community: "Community",
  treasury: "Governance",
  education: "Education",
  merchant: "Merchant",
};

export function areaById(id: GovernanceAreaId): GovernanceArea {
  const area = GOVERNANCE_AREAS.find((item) => item.id === id);
  if (!area) throw new Error("unknown governance area");
  return area;
}

export function voteEligibility(
  area: GovernanceAreaId,
  passport: Pick<PublicPassport, "reputations" | "roles">,
): { eligible: boolean; label: string } {
  const spec = areaById(area);
  const category = AREA_CATEGORY[area];
  if (category === "role") {
    return { eligible: false, label: spec.eligibility };
  }
  const score = passport.reputations[category];
  const roleHit =
    area === "tlt" && passport.roles.some((role) => /miner/i.test(role));
  const eligible = score > 0 || roleHit;
  return {
    eligible,
    label: eligible ? spec.eligibility : `Not eligible · ${spec.eligibility}`,
  };
}

export function proposalBadge(proposal: Pick<CommunityProposal, "binding" | "chainCommitment">): {
  badge: "ON-CHAIN GOVERNANCE" | "ADVISORY";
  source: DataSource;
  note: string;
} {
  if (proposal.binding === "on-chain" && proposal.chainCommitment) {
    return {
      badge: "ON-CHAIN GOVERNANCE",
      source: "indexed",
      note: "Commitment id is indexed from a full node. This light client does not re-check validator signatures.",
    };
  }
  if (proposal.binding === "on-chain") {
    return {
      badge: "ADVISORY",
      source: "community submitted",
      note: "Marked on-chain without a commitment id, so it stays an advisory poll.",
    };
  }
  return {
    badge: "ADVISORY",
    source: "community submitted",
    note: "Advisory poll. It does not execute protocol changes.",
  };
}
