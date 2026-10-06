export type { ForumPost, ForumReport } from "../community/types";

/** A like is not reputation and is not stored as activity by this module. */
export function likePost(): { appliedToReputation: false; stored: false } {
  return { appliedToReputation: false, stored: false };
}
