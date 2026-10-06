/** Bounty payment confirmation is not available. Listing stays empty here. */

export const BOUNTY_DIRECTORY = {
  status: "PLANNED" as const,
  records: [] as const,
  fabricated: false as const,
};

export function markBountyPaid(): never {
  throw new Error("PLANNED: this client does not confirm bounty payment");
}
