/** In-app trust catalog. Every row is shown. None are optional. */

export type TrustPosture =
  | "verified-locally"
  | "node-reported"
  | "operator-reported"
  | "not-proven"
  | "device-local"
  | "mixed";

export type TrustAssumption = {
  id:
    | "headers"
    | "merkle"
    | "state_proofs"
    | "rpc"
    | "indexer"
    | "community_api"
    | "wallet"
    | "passport"
    | "governance"
    | "confirmation";
  title: string;
  posture: TrustPosture;
  checks: string;
  assumption: string;
};

export const TRUST_ASSUMPTION_IDS = [
  "headers",
  "merkle",
  "state_proofs",
  "rpc",
  "indexer",
  "community_api",
  "wallet",
  "passport",
  "governance",
  "confirmation",
] as const;

export const TRUST_ASSUMPTIONS: readonly TrustAssumption[] = [
  {
    id: "headers",
    title: "Headers",
    posture: "verified-locally",
    checks:
      "The device recomputes header hashes, checks selected-parent links, and binds the spine to the genesis hash from the node the wallet is using.",
    assumption:
      "RandomX work and the GHOSTDAG blue set are computed by that full node. A node can serve a self-consistent spine that is not the public network. This client does not compare a second node.",
  },
  {
    id: "merkle",
    title: "Merkle",
    posture: "verified-locally",
    checks:
      "For a TLT transaction id, the device recomputes the pairwise SHA-256 Merkle path and the body-root fold into header.tx_root.",
    assumption:
      "The full node chooses which siblings to serve. A bad sibling fails the local check. The proof shows inclusion of that transaction id in that header. It does not prove an account balance or the absence of other transactions.",
  },
  {
    id: "state_proofs",
    title: "State proofs",
    posture: "not-proven",
    checks: "The device rejects a balance or DRC object payload that claims header_proven.",
    assumption:
      "TLT, OVL, and DRC balances and DRC ledger objects are full-node state reads. The live header this client verifies has no state-root proof for those balances. OVL execution inclusion, when a binding is present, shows an id in a block body and does not verify an EVM receipt.",
  },
  {
    id: "rpc",
    title: "RPC",
    posture: "node-reported",
    checks:
      "The wallet stores the RPC URL the user entered and checks the reported network id and genesis hash against that pairing.",
    assumption:
      "One RPC endpoint can hide transactions, balances, and votes. Bearer tokens stay in device storage. The node does not receive a mnemonic or a private key on submit. Submit sends an already signed transaction.",
  },
  {
    id: "indexer",
    title: "Indexer",
    posture: "operator-reported",
    checks: "This client does not treat an indexer as a proof source.",
    assumption:
      "An indexer, if one is added later, is an operator database. It can be incomplete or wrong. Confirmation and inclusion still require the header and Merkle checks. This build does not query an indexer for trust.",
  },
  {
    id: "community_api",
    title: "Community API",
    posture: "operator-reported",
    checks:
      "Local community search runs on device records. Seed examples are data, not a country enum, and the search box has no GPS input.",
    assumption:
      "A remote community API is not consensus. Hub, grant, mission, and analytics responses would be operator-reported. This build does not send identity exports or keys to a community API.",
  },
  {
    id: "wallet",
    title: "Wallet",
    posture: "device-local",
    checks:
      "BIP-39 generation, AES-256-GCM vault sealing, and secp256k1 signing happen on the device. Desktop uses localStorage. Phone uses SecureStore.",
    assumption:
      "The vault password and mnemonic are not RPC parameters. Watch-only sessions have an account key and no spend key. A compromised RPC cannot sign by itself. A compromised device can.",
  },
  {
    id: "passport",
    title: "Passport",
    posture: "device-local",
    checks:
      "The portable export is JSON built on the device. The builder refuses mnemonic, seed, and private-key fields. Service trust labels are attached to the bundle.",
    assumption:
      "Exporting a passport does not prove the issuer signature. This build does not re-verify hub signatures during export. Attestations are contribution evidence. They are not personhood and they are not Sybil resistance. Local preference fields stay on the device until the user copies the file.",
  },
  {
    id: "governance",
    title: "Governance",
    posture: "mixed",
    checks:
      "Each proposal view carries a derived ON-CHAIN or ADVISORY badge. Advisory records cannot be given a binding passed result. Tallies shown here are counts.",
    assumption:
      "An advisory proposal does not change consensus. An on-chain tally read from one node is node-reported. This client does not re-check ballot signatures or chamber eligibility. Shipped status requires a commit or release link, and that link is not itself a consensus proof.",
  },
  {
    id: "confirmation",
    title: "Confirmation",
    posture: "mixed",
    checks:
      "The wallet can watch a transaction until the node reports pending or confirmed, and it can verify a TLT Merkle proof locally when the node serves one.",
    assumption:
      "A single node's confirmed status is not network finality. Finality on this client is the PoW flag plus independent two-thirds OVL stake and two-thirds DRC stake, using the stake totals on the checkpoint. Those totals are not validator signature checks. If stake fields are absent, the block is not shown as finalized.",
  },
];
