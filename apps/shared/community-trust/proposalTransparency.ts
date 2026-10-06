/** Proposal transparency. The badge is derived from authority. */

export type ProposalAuthority = "on_chain" | "advisory";
export type AuthorityBadge = "ON-CHAIN" | "ADVISORY";
export type ProposalFinalResult =
  | "pending"
  | "passed"
  | "failed"
  | "expired"
  | "advisory_recorded";
export type ImplementationStatus =
  | "not_started"
  | "in_progress"
  | "shipped"
  | "wont_implement"
  | "not_applicable";

export type VoteSnapshot = {
  yes: number;
  no: number;
  abstain: number;
  no_with_veto: number;
};

export type ProposalTransparency = {
  id: string;
  authority: ProposalAuthority;
  creator: string;
  why: string;
  what_changes: string;
  expected_cost: string;
  treasury_impact: string;
  voting_period: { start_slot: number; end_slot: number };
  eligibility: string;
  current_votes: VoteSnapshot;
  final_result: ProposalFinalResult;
  implementation_status: ImplementationStatus;
  commit_links: string[];
  release_links: string[];
};

export type ProposalTransparencyView = ProposalTransparency & { badge: AuthorityBadge };

export function authorityBadge(authority: ProposalAuthority): AuthorityBadge {
  return authority === "on_chain" ? "ON-CHAIN" : "ADVISORY";
}

export function proposalView(proposal: ProposalTransparency): ProposalTransparencyView {
  return { ...proposal, badge: authorityBadge(proposal.authority) };
}

export function validateProposalTransparency(record: ProposalTransparency): void {
  const id = record.id.trim();
  const numeric = /^[0-9]{1,20}$/.test(id);
  if (!numeric && !/^[a-z0-9][a-z0-9-]{0,63}$/.test(id)) {
    throw new Error("proposal id is invalid");
  }
  required(record.creator, 128, "creator");
  required(record.why, 4000, "why");
  required(record.what_changes, 4000, "what changes");
  required(record.expected_cost, 4000, "expected cost");
  required(record.treasury_impact, 4000, "treasury impact");
  required(record.eligibility, 4000, "eligibility");
  if (record.voting_period.end_slot <= record.voting_period.start_slot) {
    throw new Error("voting period must end after it starts");
  }
  validateLinks(record.commit_links);
  validateLinks(record.release_links);
  if (
    record.implementation_status === "shipped" &&
    record.commit_links.length === 0 &&
    record.release_links.length === 0
  ) {
    throw new Error("shipped status needs a commit or release link");
  }
  if (record.authority === "advisory") {
    if (record.final_result !== "pending" && record.final_result !== "advisory_recorded") {
      throw new Error("advisory proposals stay pending or advisory_recorded");
    }
  } else {
    if (record.final_result === "advisory_recorded") {
      throw new Error("on-chain proposals do not use advisory_recorded");
    }
    if (record.implementation_status === "not_applicable") {
      throw new Error("on-chain proposals keep an implementation status");
    }
  }
}

export function exampleProposals(): ProposalTransparencyView[] {
  const onChain: ProposalTransparency = {
    id: "scaffold-on-chain-1",
    authority: "on_chain",
    creator: "agora1examplecreator",
    why: "Publish the voting window and treasury impact before any spend.",
    what_changes: "Records a community treasury intent. It does not mint TLT.",
    expected_cost: "0 TLT from emission. A DRC spend waits for a signed treasury transaction.",
    treasury_impact: "No automatic debit in this scaffold.",
    voting_period: { start_slot: 10, end_slot: 20 },
    eligibility: "Addresses the node reports as eligible. This client does not recompute the electorate.",
    current_votes: { yes: 3, no: 1, abstain: 0, no_with_veto: 0 },
    final_result: "pending",
    implementation_status: "not_started",
    commit_links: [],
    release_links: [],
  };
  const advisory: ProposalTransparency = {
    id: "advisory-note",
    authority: "advisory",
    creator: "assembly scribe",
    why: "Collect feedback before anyone drafts a consensus change.",
    what_changes: "No consensus parameter changes.",
    expected_cost: "None.",
    treasury_impact: "None.",
    voting_period: { start_slot: 1, end_slot: 2 },
    eligibility: "Anyone reading the public square.",
    current_votes: { yes: 0, no: 0, abstain: 0, no_with_veto: 0 },
    final_result: "advisory_recorded",
    implementation_status: "not_applicable",
    commit_links: [],
    release_links: [],
  };
  for (const record of [onChain, advisory]) validateProposalTransparency(record);
  return [proposalView(onChain), proposalView(advisory)];
}

function required(value: string, max: number, field: string): void {
  const trimmed = value.trim();
  if (!trimmed || [...trimmed].length > max) throw new Error(`${field} length`);
}

function validateLinks(links: string[]): void {
  if (links.length > 8) throw new Error("too many links");
  for (const link of links) {
    if (!validPublicLink(link)) throw new Error(`link ${link} is not an http(s) url`);
  }
}

function validPublicLink(link: string): boolean {
  if (link.length > 200 || /\s/.test(link) || link.includes("@") || link.includes("\\")) {
    return false;
  }
  if (link.startsWith("https://")) return link.slice("https://".length).split(/[/?#]/)[0].length > 0;
  if (link.startsWith("http://")) {
    const host = link.slice("http://".length).split(/[/?#]/)[0] ?? "";
    const bare = host.split(":")[0];
    return bare === "localhost" || bare === "127.0.0.1";
  }
  return false;
}
