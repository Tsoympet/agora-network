/**
 * Service boundaries for community features.
 * In-process adapters return no records. They must not invent payments,
 * votes, reputation, or merchant activity. The HTTP host is
 * infrastructure/community-services. It can be stale or fictional wherever
 * its trust entry says it can lie. Device packages do not import it.
 */

export type TrustBoundary = {
  service: string;
  canLieAbout: readonly string[];
  mustNotClaim: readonly string[];
};

export type HonestRead = {
  records: readonly never[];
  status: "PLANNED";
  fabricated: false;
};

export type HonestMutation = {
  status: "PLANNED";
  accepted: false;
  fabricated: false;
};

export const SERVICE_TRUST = {
  community: {
    service: "Community API",
    canLieAbout: [
      "hub display text",
      "off-chain membership",
      "fixture bundles labeled community submitted",
    ],
    mustNotClaim: [
      "consensus finality",
      "treasury spend authority",
      "that a fixture is on-chain verified",
    ],
  },
  identity: {
    service: "Identity/Passport",
    canLieAbout: ["display name", "avatar", "self-asserted roles"],
    mustNotClaim: [
      "custody of wallet keys",
      "Sybil resistance or verified personhood",
      "chain inclusion without a proof",
    ],
  },
  mission: {
    service: "Mission",
    canLieAbout: ["draft checklist text", "off-chain assignment notes"],
    mustNotClaim: ["on-chain completion", "a payment or token reward"],
  },
  grant: {
    service: "Grant",
    canLieAbout: ["draft milestone notes"],
    mustNotClaim: ["treasury disbursement", "a vote outcome", "a confirmed payment"],
  },
  bounty: {
    service: "Bounty",
    canLieAbout: ["nothing — this adapter returns no bounties"],
    mustNotClaim: ["paid status", "solver reputation", "a confirmed payment"],
  },
  academy: {
    service: "Academy",
    canLieAbout: ["local lesson progress on this device"],
    mustNotClaim: ["an on-chain certificate", "a credential another device must accept"],
  },
  event: {
    service: "Event",
    canLieAbout: ["draft RSVP intent"],
    mustNotClaim: ["attendance proof", "ticket payment"],
  },
  merchant: {
    service: "Merchant",
    canLieAbout: ["nothing — this adapter returns no merchant activity"],
    mustNotClaim: ["sales", "a received payment", "custody of merchant keys"],
  },
  forum: {
    service: "Forum",
    canLieAbout: ["local draft text"],
    mustNotClaim: ["moderation as consensus", "a vote", "reputation from a like"],
  },
  notification: {
    service: "Notification",
    canLieAbout: ["whether a push was delivered"],
    mustNotClaim: ["that a chain event occurred", "amounts inside push copy"],
  },
  indexing: {
    service: "Indexing",
    canLieAbout: ["ordering", "completeness", "derived balances", "stale mirrors"],
    mustNotClaim: ["spend authorization", "finality", "a header proof"],
  },
} as const satisfies Record<string, TrustBoundary>;

export const SERVICE_NAMES = [
  "community",
  "identity",
  "mission",
  "grant",
  "bounty",
  "academy",
  "event",
  "merchant",
  "forum",
  "notification",
  "indexing",
] as const;

export type ServiceName = (typeof SERVICE_NAMES)[number];

export type ServiceAdapter = {
  trust: TrustBoundary;
  read: () => Promise<HonestRead>;
  mutate: () => Promise<HonestMutation>;
};

function adapter(name: ServiceName): ServiceAdapter {
  const trust = SERVICE_TRUST[name];
  return {
    trust,
    async read() {
      return { records: [], status: "PLANNED", fabricated: false };
    },
    async mutate() {
      return { status: "PLANNED", accepted: false, fabricated: false };
    },
  };
}

/** Dev/in-process adapters. Empty on purpose — no sample merchants or votes. */
export function inProcessServices(): Record<ServiceName, ServiceAdapter> {
  return {
    community: adapter("community"),
    identity: adapter("identity"),
    mission: adapter("mission"),
    grant: adapter("grant"),
    bounty: adapter("bounty"),
    academy: adapter("academy"),
    event: adapter("event"),
    merchant: adapter("merchant"),
    forum: adapter("forum"),
    notification: adapter("notification"),
    indexing: adapter("indexing"),
  };
}

/**
 * Passing the wallet-signature and rate-limit gate is not personhood.
 * Canonical passport attestations are contribution evidence, not a Sybil proof.
 */
export const SYBIL_RESISTANCE = "PLANNED" as const;
