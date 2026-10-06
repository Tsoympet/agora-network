export { proposalBadge, voteEligibility } from "../community/governance";

export function recordVote(): never {
  throw new Error("PLANNED: this client does not record governance votes or invent tallies");
}
