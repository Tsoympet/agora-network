/** Public totals. Private identifiers are rejected. */

export const PUBLIC_ANALYTICS_KEYS = [
  "active_users",
  "active_developers",
  "active_merchants",
  "missions",
  "grants",
  "bounties",
  "academy",
  "events",
  "governance",
  "drc_activity",
  "ovl_contracts",
  "tlt_activity",
  "nodes",
  "miners",
  "projects",
] as const;

const PRIVATE_KEYS = [
  "user_id",
  "userId",
  "address",
  "wallet",
  "ip",
  "email",
  "device_id",
  "deviceId",
  "mnemonic",
  "private_key",
  "privateKey",
  "session",
  "gps",
  "latitude",
  "longitude",
  "name",
  "account",
];

export type PublicAnalyticsKey = (typeof PUBLIC_ANALYTICS_KEYS)[number];
export type PublicAnalyticsAggregates = Record<PublicAnalyticsKey, number>;

export type PublicAnalyticsReport = {
  maturity: "scaffold";
  private_user_analytics: false;
  aggregates: PublicAnalyticsAggregates;
  assumption: string;
};

const ASSUMPTION =
  "Counts are public totals only. This build ships zeros until a public aggregator exists. No per-user series is accepted.";

export function scaffoldPublicAnalytics(): PublicAnalyticsReport {
  const aggregates = {} as PublicAnalyticsAggregates;
  for (const key of PUBLIC_ANALYTICS_KEYS) aggregates[key] = 0;
  return {
    maturity: "scaffold",
    private_user_analytics: false,
    aggregates,
    assumption: ASSUMPTION,
  };
}

export function parsePublicAnalytics(raw: unknown): PublicAnalyticsAggregates {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
    throw new Error("analytics must be an object");
  }
  const record = raw as Record<string, unknown>;
  for (const key of Object.keys(record)) {
    if (PRIVATE_KEYS.includes(key)) throw new Error(`private field ${key}`);
    if (!(PUBLIC_ANALYTICS_KEYS as readonly string[]).includes(key)) {
      throw new Error(`unknown field ${key}`);
    }
  }
  const aggregates = {} as PublicAnalyticsAggregates;
  for (const key of PUBLIC_ANALYTICS_KEYS) {
    if (!(key in record)) throw new Error(`missing field ${key}`);
    const value = record[key];
    if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) {
      throw new Error(`${key} must be a non-negative safe integer`);
    }
    aggregates[key] = value;
  }
  return aggregates;
}

export const ANALYTICS_LABELS: Record<PublicAnalyticsKey, string> = {
  active_users: "Active users",
  active_developers: "Active developers",
  active_merchants: "Active merchants",
  missions: "Missions",
  grants: "Grants",
  bounties: "Bounties",
  academy: "Academy",
  events: "Events",
  governance: "Governance",
  drc_activity: "DRC activity",
  ovl_contracts: "OVL contracts",
  tlt_activity: "TLT activity",
  nodes: "Nodes",
  miners: "Miners",
  projects: "Projects",
};
