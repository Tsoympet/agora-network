/**
 * Where an Agora fact is allowed to live.
 * Blockchain is reserved for facts a device can verify or that consensus commits.
 * Node-reported balances and community profiles are not that store.
 */

export const DATA_STORES = [
  "BLOCKCHAIN",
  "COMMUNITY",
  "INDEXER",
  "USER_PRIVATE",
  "CACHE",
  "NOTIFICATIONS",
] as const;

export type DataStore = (typeof DATA_STORES)[number];

export const STORE_HOLDS: Record<DataStore, readonly string[]> = {
  BLOCKCHAIN: [
    "header hashes and selected-parent links the device recomputes",
    "TLT Merkle inclusion the device recomputes",
    "signed transaction bytes before they are submitted",
    "consensus-committed registry roots once a full node has accepted them",
  ],
  COMMUNITY: [
    "passport display names and avatars",
    "mission and grant drafts",
    "forum text",
    "merchant directory copy",
    "fixture bundles labeled community submitted",
  ],
  INDEXER: [
    "node-reported balances",
    "ordering and completeness of lists",
    "treasury rows read from agora_getProtocolTreasuries",
    "community records mirrored from a service",
  ],
  USER_PRIVATE: [
    "mnemonic and seed",
    "vault password",
    "PIN hash",
    "RPC bearer token",
    "spend-session secret",
  ],
  CACHE: [
    "tip snapshots",
    "offline community views",
    "QR module matrices",
  ],
  NOTIFICATIONS: [
    "on-device notification preferences",
    "push copy with amounts removed",
  ],
};

export const FACT_PLACEMENT = {
  "header.hash": "BLOCKCHAIN",
  "tlt.merkle": "BLOCKCHAIN",
  "signed-tx": "BLOCKCHAIN",
  "node-reported-balance": "INDEXER",
  "community-profile": "COMMUNITY",
  "merchant-directory": "COMMUNITY",
  mnemonic: "USER_PRIVATE",
  "vault-password": "USER_PRIVATE",
  "rpc-token": "USER_PRIVATE",
  "pin-hash": "USER_PRIVATE",
  "tip-cache": "CACHE",
  "notification-pref": "NOTIFICATIONS",
} as const;

export type PlacedFact = keyof typeof FACT_PLACEMENT;

export function placementFor(fact: PlacedFact): DataStore {
  return FACT_PLACEMENT[fact];
}

/** Private facts never share a store with verifiable chain data. */
export function isUserPrivate(fact: PlacedFact): boolean {
  return placementFor(fact) === "USER_PRIVATE";
}
