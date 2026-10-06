/**
 * Device-to-device pairing for the Agora light client.
 *
 * Watch payloads carry public identifiers only. Restore payloads carry a BIP-39
 * mnemonic and exist so a second device can spend — anyone who reads that
 * payload can spend. Neither payload is an account on a node or a cloud login.
 */
import { normalizeNetworkId, type AgoraNetworkId } from "./network";
import { classifyRpcReach, validateRpcUrl } from "./rpcEndpoint";
import {
  assertXpubWatchOnly,
  deriveAccount,
  derivePublicAccount,
  exportAccountXpub,
  validateMnemonic,
} from "./wallet";

export const PAIRING_VERSION = 1 as const;
export const WATCH_KIND = "agora-light-watch-v1";
export const RESTORE_KIND = "agora-light-restore-v1";
export const MAX_PAIRING_CHARS = 3072;

const GENESIS_HEX = /^[0-9a-f]{64}$/;
const NETWORK_WORDS = new Set([
  "mainnet",
  "main",
  "testnet",
  "test",
  "devnet",
  "dev",
  "local",
]);

const WATCH_KEYS = new Set([
  "v",
  "kind",
  "network",
  "genesis",
  "account",
  "xpub",
  "tlt_receive",
  "tlt_receive_hex",
  "tlt_change",
  "tlt_change_hex",
  "ovl_account",
  "drc_account",
  "rpc_url",
]);

const RESTORE_KEYS = new Set(["v", "kind", "network", "genesis", "mnemonic"]);

export const PAIRING_GUIDE_STEPS = [
  "Pairing is device-to-device. The mnemonic and private keys never go to a node or an Agora cloud account.",
  "Use the same network on both devices. Compare the genesis hash before importing. If it differs, stop — the phone may be talking to a different chain.",
  "After import, the phone checks the selected-parent header spine locally back to that genesis. A window that does not reach genesis is rejected.",
  "Away from home, set a custom RPC URL (https preferred). The other device does not need to stay online. Both modes query that RPC on their own.",
  "Watch-only is the travel mode. The payload has public addresses, OVL and DRC account ids, and the account xpub. The phone can read balances and cannot spend.",
  "Same-spend restore copies the BIP-39 mnemonic (typed or a reveal-once code). Anyone who sees the words can spend. Hide the code as soon as the other device has it.",
] as const;

export type WatchOnlyWallet = {
  mode: "watch";
  network: AgoraNetworkId;
  genesis: string;
  account: number;
  xpub: string;
  tltReceive: string;
  tltReceiveHex: string;
  tltChange: string;
  tltChangeHex: string;
  ovlAccount: string;
  drcAccount: string;
  rpcUrl: string | null;
};

export type RestorePairing = {
  mode: "restore";
  network: AgoraNetworkId;
  genesis: string;
  mnemonic: string;
};

export type ParsedPairing =
  | { kind: "watch"; watch: WatchOnlyWallet }
  | { kind: "restore"; restore: RestorePairing };

type WatchWire = {
  v: typeof PAIRING_VERSION;
  kind: typeof WATCH_KIND;
  network: AgoraNetworkId;
  genesis: string;
  account: number;
  xpub: string;
  tlt_receive: string;
  tlt_receive_hex: string;
  tlt_change: string;
  tlt_change_hex: string;
  ovl_account: string;
  drc_account: string;
  rpc_url?: string;
};

type RestoreWire = {
  v: typeof PAIRING_VERSION;
  kind: typeof RESTORE_KIND;
  network: AgoraNetworkId;
  genesis: string;
  mnemonic: string;
};

export type SpendSession = { mode: "spend"; mnemonic: string };
export type WalletSession = SpendSession | WatchOnlyWallet;

function malformed(): Error {
  return new Error("malformed pairing payload");
}

function normalizeMnemonic(mnemonic: string): string {
  const phrase = mnemonic.trim().toLowerCase().replace(/\s+/g, " ");
  if (!validateMnemonic(phrase)) {
    throw new Error("invalid BIP-39 mnemonic");
  }
  return phrase;
}

function requireNetwork(raw: unknown): AgoraNetworkId {
  if (typeof raw !== "string" || !NETWORK_WORDS.has(raw.trim().toLowerCase())) {
    throw malformed();
  }
  return normalizeNetworkId(raw);
}

function requireGenesis(raw: unknown): string {
  if (typeof raw !== "string") throw malformed();
  const hex = raw.trim().toLowerCase().replace(/^0x/, "");
  if (!GENESIS_HEX.test(hex)) throw malformed();
  return hex;
}

function requireAccount(raw: unknown): number {
  if (typeof raw !== "number" || !Number.isInteger(raw) || raw < 0 || raw > 20) {
    throw malformed();
  }
  return raw;
}

function ownKeys(value: object): string[] {
  return Object.keys(value);
}

function assertExactKeys(value: object, allowed: Set<string>): void {
  for (const key of ownKeys(value)) {
    if (!allowed.has(key)) throw malformed();
  }
}

function watchFromParts(
  xpub: string,
  network: AgoraNetworkId,
  genesis: string,
  account: number,
  rpcUrl: string | null,
): WatchOnlyWallet {
  assertXpubWatchOnly(xpub);
  const receive = derivePublicAccount(xpub, account, network, 0);
  const change = derivePublicAccount(xpub, account, network, 1);
  return {
    mode: "watch",
    network,
    genesis,
    account,
    xpub,
    tltReceive: receive.addressBech32,
    tltReceiveHex: receive.addressHex,
    tltChange: change.addressBech32,
    tltChangeHex: change.addressHex,
    ovlAccount: receive.addressHex,
    drcAccount: receive.addressHex,
    rpcUrl,
  };
}

function wireFromWatch(watch: WatchOnlyWallet): WatchWire {
  const wire: WatchWire = {
    v: PAIRING_VERSION,
    kind: WATCH_KIND,
    network: watch.network,
    genesis: watch.genesis,
    account: watch.account,
    xpub: watch.xpub,
    tlt_receive: watch.tltReceive,
    tlt_receive_hex: watch.tltReceiveHex,
    tlt_change: watch.tltChange,
    tlt_change_hex: watch.tltChangeHex,
    ovl_account: watch.ovlAccount,
    drc_account: watch.drcAccount,
  };
  if (watch.rpcUrl) wire.rpc_url = watch.rpcUrl;
  return wire;
}

function publicRpcHint(rpcUrl: string | null | undefined): string | null {
  if (!rpcUrl || !rpcUrl.trim()) return null;
  const normalized = validateRpcUrl(rpcUrl);
  if (classifyRpcReach(normalized) === "loopback") return null;
  return normalized;
}

/** Build a watch-only payload from a spend mnemonic. Loopback RPC is omitted. */
export function buildWatchPairing(args: {
  mnemonic: string;
  network: string;
  genesis: string;
  account?: number;
  rpcUrl?: string | null;
}): WatchOnlyWallet {
  const network = requireNetwork(args.network);
  const genesis = requireGenesis(args.genesis);
  const account = args.account ?? 0;
  requireAccount(account);
  const phrase = normalizeMnemonic(args.mnemonic);
  const xpub = exportAccountXpub(phrase);
  const spend = deriveAccount(phrase, account, "", network, 0);
  const watch = watchFromParts(xpub, network, genesis, account, publicRpcHint(args.rpcUrl));
  if (watch.tltReceiveHex !== spend.addressHex || watch.tltReceive !== spend.addressBech32) {
    throw new Error("xpub does not match the spend account");
  }
  const change = deriveAccount(phrase, account, "", network, 1);
  if (watch.tltChangeHex !== change.addressHex) {
    throw new Error("xpub does not match the change account");
  }
  return watch;
}

/** Rebuild a watch payload from an already-imported public wallet. */
export function watchWithRpc(watch: WatchOnlyWallet, rpcUrl?: string | null): WatchOnlyWallet {
  return watchFromParts(
    watch.xpub,
    watch.network,
    watch.genesis,
    watch.account,
    publicRpcHint(rpcUrl === undefined ? watch.rpcUrl : rpcUrl),
  );
}

export function buildRestorePairing(args: {
  mnemonic: string;
  network: string;
  genesis: string;
}): RestorePairing {
  return {
    mode: "restore",
    network: requireNetwork(args.network),
    genesis: requireGenesis(args.genesis),
    mnemonic: normalizeMnemonic(args.mnemonic),
  };
}

export function serializePairing(payload: WatchOnlyWallet | RestorePairing): string {
  const text =
    payload.mode === "watch"
      ? JSON.stringify(wireFromWatch(payload))
      : JSON.stringify({
          v: PAIRING_VERSION,
          kind: RESTORE_KIND,
          network: payload.network,
          genesis: payload.genesis,
          mnemonic: payload.mnemonic,
        } satisfies RestoreWire);
  if (text.length > MAX_PAIRING_CHARS) throw malformed();
  return text;
}

function rejectPrivateMarkers(text: string): void {
  if (/xprv|yprv|zprv|xpriv/i.test(text)) throw malformed();
}

function parseWatch(value: Record<string, unknown>): WatchOnlyWallet {
  assertExactKeys(value, WATCH_KEYS);
  if (value.v !== PAIRING_VERSION || value.kind !== WATCH_KIND) throw malformed();
  rejectPrivateMarkers(JSON.stringify(value));
  const network = requireNetwork(value.network);
  const genesis = requireGenesis(value.genesis);
  const account = requireAccount(value.account);
  if (typeof value.xpub !== "string") throw malformed();
  let rpcUrl: string | null = null;
  if (value.rpc_url !== undefined) {
    if (typeof value.rpc_url !== "string") throw malformed();
    rpcUrl = validateRpcUrl(value.rpc_url);
    if (classifyRpcReach(rpcUrl) === "loopback") throw malformed();
  }
  const watch = watchFromParts(value.xpub, network, genesis, account, rpcUrl);
  const expect: Array<[unknown, string]> = [
    [value.tlt_receive, watch.tltReceive],
    [value.tlt_receive_hex, watch.tltReceiveHex],
    [value.tlt_change, watch.tltChange],
    [value.tlt_change_hex, watch.tltChangeHex],
    [value.ovl_account, watch.ovlAccount],
    [value.drc_account, watch.drcAccount],
  ];
  for (const [got, want] of expect) {
    if (got !== want) throw malformed();
  }
  return watch;
}

function parseRestore(value: Record<string, unknown>): RestorePairing {
  assertExactKeys(value, RESTORE_KEYS);
  if (value.v !== PAIRING_VERSION || value.kind !== RESTORE_KIND) throw malformed();
  if (typeof value.mnemonic !== "string") throw malformed();
  return {
    mode: "restore",
    network: requireNetwork(value.network),
    genesis: requireGenesis(value.genesis),
    mnemonic: normalizeMnemonic(value.mnemonic),
  };
}

/** Accept one pairing payload. Unknown keys, private keys, and bad shapes are rejected. */
export function parsePairingPayload(raw: string): ParsedPairing {
  if (typeof raw !== "string") throw malformed();
  const text = raw.trim();
  if (!text || text.length > MAX_PAIRING_CHARS) throw malformed();
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw malformed();
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw malformed();
  const value = parsed as Record<string, unknown>;
  if (value.kind === WATCH_KIND) {
    return { kind: "watch", watch: parseWatch(value) };
  }
  if (value.kind === RESTORE_KIND) {
    return { kind: "restore", restore: parseRestore(value) };
  }
  throw malformed();
}

/**
 * Block import until the connected node reports the same network and genesis
 * as the pairing payload. Missing node data fails closed.
 */
export function pairingImportBlocker(
  payload: { network: string; genesis: string },
  node: { network: string | null; genesis: string | null },
): string | null {
  if (!node.network || !node.genesis) {
    return "Connect to a node, then match its network and genesis before importing.";
  }
  if (normalizeNetworkId(payload.network) !== normalizeNetworkId(node.network)) {
    return "Pairing network does not match this node.";
  }
  if (requireGenesis(payload.genesis) !== requireGenesis(node.genesis)) {
    return "Pairing genesis does not match this node. Refusing a different chain.";
  }
  return null;
}

/**
 * Spend only from an explicit spend session. Watch-only never calls `build`.
 */
export async function signSpend<T>(
  wallet: WalletSession,
  build: (mnemonic: string) => Promise<T>,
): Promise<T> {
  if (wallet.mode !== "spend") {
    throw new Error("watch-only wallet cannot sign or spend");
  }
  const phrase = normalizeMnemonic(wallet.mnemonic);
  return build(phrase);
}
