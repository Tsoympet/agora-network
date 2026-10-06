import {
  REPUTATION_CATEGORIES,
  type Badge,
  type PrivatePassportProfile,
  type PublicPassport,
  type ReputationCategory,
  type ReputationScores,
} from "./types.ts";

export const REPUTATION_EVENTS = [
  "mission_complete",
  "grant_milestone",
  "vote_recorded",
  "academy_cert",
  "like",
] as const;

export type ReputationEventKind = (typeof REPUTATION_EVENTS)[number];

export type ReputationEvent = {
  kind: ReputationEventKind;
  category: ReputationCategory;
  evidenceId: string;
};

const EVENT_CATEGORY: Record<Exclude<ReputationEventKind, "like">, ReputationCategory> = {
  mission_complete: "Community",
  grant_milestone: "Builder",
  vote_recorded: "Governance",
  academy_cert: "Education",
};

export function emptyScores(): ReputationScores {
  return {
    Builder: 0,
    Community: 0,
    Merchant: 0,
    Security: 0,
    Mining: 0,
    Governance: 0,
    Education: 0,
  };
}

export function overallScore(scores: ReputationScores): number {
  const total = REPUTATION_CATEGORIES.reduce((sum, key) => sum + scores[key], 0);
  return Math.floor(total / REPUTATION_CATEGORIES.length);
}

/**
 * Likes are social noise. Only verifiable contribution events move a score,
 * and only in the category the event names.
 */
export function applyReputationEvent(
  scores: ReputationScores,
  event: ReputationEvent,
): { scores: ReputationScores; overall: number; applied: boolean } {
  if (event.kind === "like" || !event.evidenceId.trim()) {
    return { scores: { ...scores }, overall: overallScore(scores), applied: false };
  }
  const next = { ...scores };
  const category = event.category || EVENT_CATEGORY[event.kind];
  next[category] += 1;
  return { scores: next, overall: overallScore(next), applied: true };
}

export function transferBadge(_badge: Badge, _to: string): never {
  throw new Error("passport badges are non-transferable identity credentials");
}

export function splitPassport(input: {
  publicProfile: PublicPassport;
  privateProfile: PrivatePassportProfile;
}): { publicProfile: PublicPassport; privateProfile: PrivatePassportProfile } {
  if (input.privateProfile.consensus !== false) {
    throw new Error("private passport profile cannot be marked consensus");
  }
  const publicProfile: PublicPassport = {
    schemaVersion: 1,
    address: input.publicProfile.address,
    username: input.publicProfile.username,
    avatarHash: input.publicProfile.avatarHash,
    roles: [...input.publicProfile.roles],
    reputations: { ...input.publicProfile.reputations },
    overall: input.publicProfile.overall,
    badges: input.publicProfile.badges.map((badge) => ({ ...badge, nonTransferable: true })),
    contributionCounts: { ...input.publicProfile.contributionCounts },
    source: input.publicProfile.source,
  };
  const privateProfile: PrivatePassportProfile = {
    schemaVersion: 1,
    address: input.privateProfile.address,
    email: input.privateProfile.email,
    language: input.privateProfile.language,
    storage: input.privateProfile.storage,
    consensus: false,
  };
  const serialized = JSON.stringify(publicProfile);
  if (privateProfile.email && serialized.includes(privateProfile.email)) {
    throw new Error("public passport leaked a private email");
  }
  return { publicProfile, privateProfile };
}
