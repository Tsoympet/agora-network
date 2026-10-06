/**
 * Milestone acceptance records evidence only.
 * This client never sets a disbursement, even if a caller asks for one.
 */

import type { Grant } from "../community/types";

export function acceptGrantMilestone(
  grant: Grant,
  milestoneId: string,
  evidenceHash: string,
): Grant {
  if (!/^[0-9a-f]{64}$/i.test(evidenceHash)) {
    throw new Error("deliverable hash required");
  }
  const milestone = grant.milestones.find((item) => item.id === milestoneId);
  if (!milestone) throw new Error("unknown milestone");
  if (milestone.accepted) throw new Error("milestone already accepted");
  return {
    ...grant,
    disbursesFunds: false,
    milestones: grant.milestones.map((item) =>
      item.id === milestoneId ? { ...item, accepted: true, evidenceHash } : item,
    ),
  };
}

export const GRANT_DISBURSEMENT = "PLANNED" as const;
