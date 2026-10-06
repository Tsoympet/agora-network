import type { VerificationStatus } from "./verificationStatus";

export type FeatureDomain = "network" | "tlt" | "drc" | "ovl";

export type FeatureCapability = {
  id: string;
  domain: FeatureDomain;
  title: string;
  detail: string;
  /** Default label before RPC probe refines availability. */
  status: VerificationStatus;
  /** When set, a successful probe marks the surface as node-reported (or local if noted). */
  rpcMethod?: string;
  /** Static surfaces that never gain an RPC probe (local crypto only). */
  localOnly?: boolean;
};

export const LIGHT_FEATURE_MATRIX: FeatureCapability[] = [
  {
    id: "network.rpc",
    domain: "network",
    title: "RPC endpoint",
    detail: "User-chosen JSON-RPC URL and optional bearer token.",
    status: "node-reported",
  },
  {
    id: "network.genesis",
    domain: "network",
    title: "Genesis binding",
    detail: "Wallet compares agora_getNodeInfo.genesis_hash to the paired genesis.",
    status: "verified-locally",
    localOnly: true,
  },
  {
    id: "network.header_spine",
    domain: "network",
    title: "Selected-parent header spine",
    detail: "Recomputed header hashes and ancestry to genesis.",
    status: "verified-locally",
    rpcMethod: "agora_getLightHeaders",
    localOnly: true,
  },
  {
    id: "network.finality_quorum",
    domain: "network",
    title: "OVL / DRC finality quorums",
    detail: "Two-thirds stake math on checkpoint totals (not signature checks).",
    status: "verified-locally",
    rpcMethod: "agora_getLightHeaders",
    localOnly: true,
  },
  {
    id: "network.validator_set",
    domain: "network",
    title: "Validator sets",
    detail: "Independent OVL and DRC stake tables.",
    status: "node-reported",
    rpcMethod: "agora_getValidatorSet",
  },
  {
    id: "network.native_supply",
    domain: "network",
    title: "Native supply / fee burn",
    detail: "Issued, burned, and net supply per native asset.",
    status: "node-reported",
    rpcMethod: "agora_getNativeAssetSupply",
  },
  {
    id: "tlt.receive",
    domain: "tlt",
    title: "Receive address",
    detail: "BIP-44 m/44'/8888'/0'/0/0 Bech32m receive.",
    status: "verified-locally",
    localOnly: true,
  },
  {
    id: "tlt.utxo_list",
    domain: "tlt",
    title: "UTXO list",
    detail: "Spendable outputs for an address.",
    status: "node-reported",
    rpcMethod: "agora_getUtxos",
  },
  {
    id: "tlt.send",
    domain: "tlt",
    title: "Coin-selection send",
    detail: "Device signs; node admits via agora_submitTransaction.",
    status: "verified-locally",
    rpcMethod: "agora_submitTransaction",
    localOnly: true,
  },
  {
    id: "tlt.fee_estimate",
    domain: "tlt",
    title: "Fee estimate",
    detail: "Suggested relay fee from the node.",
    status: "node-reported",
    rpcMethod: "agora_estimateFee",
  },
  {
    id: "tlt.history",
    domain: "tlt",
    title: "Transaction history",
    detail: "Point lookups for known tx ids (mempool + confirmed).",
    status: "node-reported",
    rpcMethod: "agora_getTransaction",
  },
  {
    id: "tlt.merkle",
    domain: "tlt",
    title: "Merkle inclusion verify",
    detail: "agora_getTltInclusionProof recomputed on device.",
    status: "verified-locally",
    rpcMethod: "agora_getTltInclusionProof",
    localOnly: true,
  },
  {
    id: "tlt.covenant",
    domain: "tlt",
    title: "Covenant / HTLC spends",
    detail: "Not exposed until block-accepted covenant lanes ship on this branch.",
    status: "unavailable",
  },
  {
    id: "drc.policy",
    domain: "drc",
    title: "Account policy",
    detail: "Destination tag, DepositAuth, master-key disable flags.",
    status: "node-reported",
    rpcMethod: "agora_getDrcAccountPolicy",
  },
  {
    id: "drc.tags",
    domain: "drc",
    title: "Source / destination tags",
    detail: "Carried on payments and surfaced on receipts.",
    status: "node-reported",
    rpcMethod: "agora_getDrcPayment",
  },
  {
    id: "drc.deposit_auth",
    domain: "drc",
    title: "DepositAuth preauth",
    detail: "Exact recipient/source grant lookup.",
    status: "node-reported",
    rpcMethod: "agora_getDrcDepositPreauth",
  },
  {
    id: "drc.keys",
    domain: "drc",
    title: "Regular keys",
    detail: "Account key rotation state.",
    status: "node-reported",
    rpcMethod: "agora_getDrcAccountKeys",
  },
  {
    id: "drc.multisign",
    domain: "drc",
    title: "Signer list / multisign",
    detail: "Weighted authorization quorum.",
    status: "node-reported",
    rpcMethod: "agora_getDrcAccountSignerList",
  },
  {
    id: "drc.tickets",
    domain: "drc",
    title: "Tickets",
    detail: "Sequence tickets and ticket-set objects.",
    status: "node-reported",
    rpcMethod: "agora_getDrcTicket",
  },
  {
    id: "drc.escrow",
    domain: "drc",
    title: "Escrow",
    detail: "Live escrow and settlement receipts.",
    status: "node-reported",
    rpcMethod: "agora_getDrcEscrow",
  },
  {
    id: "drc.checks",
    domain: "drc",
    title: "Checks",
    detail: "Live checks and cash receipts.",
    status: "node-reported",
    rpcMethod: "agora_getDrcCheck",
  },
  {
    id: "drc.channels",
    domain: "drc",
    title: "Payment channels",
    detail: "Channel point lookup and receipts.",
    status: "node-reported",
    rpcMethod: "agora_getDrcPaymentChannel",
  },
  {
    id: "drc.trust_lines",
    domain: "drc",
    title: "Trust lines",
    detail: "Issued-asset holder lines.",
    status: "node-reported",
    rpcMethod: "agora_getDrcTrustLine",
  },
  {
    id: "drc.issued_controls",
    domain: "drc",
    title: "Issued-asset freeze / clawback",
    detail: "Live policy and issuer controls.",
    status: "node-reported",
    rpcMethod: "agora_getDrcIssuedAssetPolicy",
  },
  {
    id: "drc.owner_objects",
    domain: "drc",
    title: "Owner object pages",
    detail: "Bounded agora_getDrcAccountObjects pagination.",
    status: "node-reported",
    rpcMethod: "agora_getDrcAccountObjects",
  },
  {
    id: "drc.spend",
    domain: "drc",
    title: "Typed sign + submit",
    detail: "Device-built DRC envelopes; watch-only cannot sign.",
    status: "verified-locally",
    localOnly: true,
  },
  {
    id: "ovl.transfer",
    domain: "ovl",
    title: "Native OVL transfer",
    detail: "Signed agora_submitAccountTransfer for OVL.",
    status: "verified-locally",
    rpcMethod: "agora_submitAccountTransfer",
    localOnly: true,
  },
  {
    id: "ovl.balance",
    domain: "ovl",
    title: "Balance / nonce",
    detail: "agora_getNativeBalances account module.",
    status: "node-reported",
    rpcMethod: "agora_getNativeBalances",
  },
  {
    id: "ovl.staking",
    domain: "ovl",
    title: "Staking / validators",
    detail: "Validator set and reward pool reads.",
    status: "node-reported",
    rpcMethod: "agora_getValidatorSet",
  },
  {
    id: "ovl.evm_deploy",
    domain: "ovl",
    title: "EVM contract deploy",
    detail: "Not claimed on this wallet base (no fake eth_call surface).",
    status: "unavailable",
  },
];

export function matrixWithRpcAvailability(
  base: FeatureCapability[],
  availableMethods: ReadonlySet<string>,
): FeatureCapability[] {
  return base.map((row) => {
    if (row.status === "unavailable") return row;
    if (row.localOnly) return row;
    if (!row.rpcMethod) return row;
    if (availableMethods.has(row.rpcMethod)) {
      return {
        ...row,
        status: row.id === "tlt.merkle" || row.id === "network.header_spine"
          ? "verified-locally"
          : "node-reported",
      };
    }
    return { ...row, status: "unavailable" as VerificationStatus };
  });
}
