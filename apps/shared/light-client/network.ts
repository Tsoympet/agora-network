/**
 * Network identity helpers for wallets / explorer UI.
 *
 * Node `agora_getNodeInfo.network` is the source of truth (mainnet | testnet | dev).
 * Address HRPs: agora / agoratest / agoradev.
 */

export type AgoraNetworkId = "mainnet" | "testnet" | "devnet";

/** Normalize RPC / env network strings to a stable id. */
export function normalizeNetworkId(
  raw: string | null | undefined,
): AgoraNetworkId {
  const s = (raw ?? "").trim().toLowerCase();
  if (s === "mainnet" || s === "main") return "mainnet";
  if (s === "testnet" || s === "test") return "testnet";
  // "dev", "devnet", "local", empty, unknown → devnet (agoradev HRP)
  return "devnet";
}

/** Human label for status chrome (Devnet / Testnet / Mainnet). */
export function networkLabel(raw: string | null | undefined): string {
  switch (normalizeNetworkId(raw)) {
    case "mainnet":
      return "Mainnet";
    case "testnet":
      return "Testnet";
    default:
      return "Devnet";
  }
}

/**
 * Accent color for the network indicator (brand-aligned, not purple glow).
 * Mainnet gold · Testnet cyan · Devnet steel.
 */
export function networkAccent(raw: string | null | undefined): string {
  switch (normalizeNetworkId(raw)) {
    case "mainnet":
      return "#C59835";
    case "testnet":
      return "#06BBDF";
    default:
      return "#8B9BB4";
  }
}

/** Wallet / Bech32 network id derived from live node info. */
export function walletNetworkFromNode(
  raw: string | null | undefined,
): AgoraNetworkId {
  return normalizeNetworkId(raw);
}

/** Short receive-address hint for the active network. */
export function networkHrpHint(raw: string | null | undefined): string {
  switch (normalizeNetworkId(raw)) {
    case "mainnet":
      return "agora1…";
    case "testnet":
      return "agoratest1…";
    default:
      return "agoradev1…";
  }
}
