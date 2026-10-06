import { FormEvent, useEffect, useMemo, useState } from "react";
import { ArchitecturePanel } from "./components/ArchitecturePanel";
import { useVaultSessionTimeout } from "./components/useVaultSessionTimeout";
import { DEFAULT_SESSION_TIMEOUT_MS } from "../../shared/security/desktop";
import {
  addressBech32FromMnemonic,
  clearPersistedVault,
  createLightClient,
  generateMnemonic,
  loadRpcEndpoint,
  loadRpcToken,
  loadSealedVault,
  localStorageVault,
  networkAccent,
  networkHrpHint,
  networkLabel,
  openVault,
  parseAddress,
  persistSealedVault,
  RPC_ENDPOINT_STORAGE_KEY,
  RPC_TOKEN_STORAGE_KEY,
  sealVault,
  sendTransfer,
  shortAddress,
  readNativeBalances,
  saveRpcEndpoint,
  saveRpcToken,
  shortHash,
  signSpend,
  startTipSync,
  walletNetworkFromNode,
  watchTransaction,
  type LightNodeInfo,
  type LightTxLookup,
  type LightUtxo,
  type NativeBalances,
  type TipSyncSnapshot,
  type WatchOnlyWallet,
} from "../../shared/light-client";
import { CommunityTrustPanel } from "./components/CommunityTrustPanel";
import { DocsAboutPanel } from "./components/DocsAboutPanel";
import { FeatureSurfacesPanel } from "./components/FeatureSurfacesPanel";
import { GovernancePanel } from "./components/GovernancePanel";
import { LightClientPanel } from "./components/LightClientPanel";
import {
  CommunityScreens,
  initialNotificationPrefs,
  useCommunityClient,
} from "./community/Screens";
import type { LightProtocolTreasuries } from "../../shared/light-client";
import { PairingPanel } from "./components/PairingPanel";

const DESKTOP_LANES = [
  "HOME",
  "WALLET",
  "DRC",
  "OVL",
  "TLT",
  "SWAP",
  "ACTIVITY",
  "PASSPORT",
  "COMMUNITY",
  "MISSIONS",
  "ACADEMY",
  "GRANTS",
  "BOUNTIES",
  "GUILDS",
  "MERCHANTS",
  "EVENTS",
  "ASSEMBLY",
  "TREASURY",
  "EXPLORER",
  "SETTINGS",
] as const;

type DesktopLane = (typeof DESKTOP_LANES)[number];

const vaultStorage = localStorageVault();
const rpcStorage = localStorageVault(RPC_ENDPOINT_STORAGE_KEY);
const rpcTokenStorage = localStorageVault(RPC_TOKEN_STORAGE_KEY);

function resolveRpcUrl(): string {
  return (
    (import.meta.env.VITE_AGORA_RPC_URL as string | undefined) ||
    "http://127.0.0.1:8545/rpc"
  );
}

function resolveRpcToken(): string | undefined {
  const t = (import.meta.env.VITE_AGORA_RPC_TOKEN as string | undefined)?.trim();
  return t || undefined;
}

const pollMs = Number(import.meta.env.VITE_AGORA_POLL_MS) || 2000;

async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

export function App() {
  const [rpcUrl, setRpcUrl] = useState(resolveRpcUrl);
  const [rpcToken, setRpcToken] = useState(resolveRpcToken() ?? "");
  const [rpcHydrated, setRpcHydrated] = useState(false);
  const client = useMemo(
    () =>
      createLightClient({
        rpcUrl,
        rpcToken: rpcToken.trim() || undefined,
      }),
    [rpcUrl, rpcToken],
  );
  const [snap, setSnap] = useState<TipSyncSnapshot>({
    status: "idle",
    tips: [],
    tipBlocks: [],
    error: null,
    updatedAt: null,
  });
  const [nodeInfo, setNodeInfo] = useState<LightNodeInfo | null>(null);
  const [address, setAddress] = useState("");
  const [balance, setBalance] = useState<number | null>(null);
  const [nativeBalances, setNativeBalances] = useState<NativeBalances | null>(null);
  const [utxos, setUtxos] = useState<LightUtxo[]>([]);
  const [walletError, setWalletError] = useState<string | null>(null);
  const [walletBusy, setWalletBusy] = useState(false);

  const [mnemonic, setMnemonic] = useState("");
  const [mnemonicVisible, setMnemonicVisible] = useState(false);
  const [vaultPassword, setVaultPassword] = useState("");
  const [vaultHasBlob, setVaultHasBlob] = useState(false);
  const [vaultUnlocked, setVaultUnlocked] = useState(false);
  const [sessionTimeoutMs, setSessionTimeoutMs] = useState(DEFAULT_SESSION_TIMEOUT_MS);
  useVaultSessionTimeout(vaultUnlocked, onLockVault, sessionTimeoutMs);
  const [vaultBusy, setVaultBusy] = useState(false);
  const [vaultMsg, setVaultMsg] = useState<string | null>(null);
  const [toAddress, setToAddress] = useState("");
  const [amount, setAmount] = useState("1");
  const [fee, setFee] = useState("1");
  const [sendBusy, setSendBusy] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const [lastTxId, setLastTxId] = useState<string | null>(null);
  const [txLookup, setTxLookup] = useState<LightTxLookup | null>(null);
  const [receiveBech32, setReceiveBech32] = useState<string | null>(null);
  const [receiveHex, setReceiveHex] = useState<string | null>(null);
  const [changeBalance, setChangeBalance] = useState<number | null>(null);
  const [copyHint, setCopyHint] = useState<string | null>(null);
  const [lane, setLane] = useState<DesktopLane>("HOME");
  const [chainTreasuries, setChainTreasuries] = useState<LightProtocolTreasuries | null>(null);
  const [notifications, setNotifications] = useState(initialNotificationPrefs);
  const community = useCommunityClient(
    (import.meta.env.VITE_AGORA_COMMUNITY_URL as string | undefined) || null,
  );
  const [watchWallet, setWatchWallet] = useState<WatchOnlyWallet | null>(null);

  useEffect(() => {
    if (!rpcHydrated) return;
    return startTipSync({ client, pollMs, onUpdate: setSnap });
  }, [client, rpcHydrated]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const saved = await loadRpcEndpoint(rpcStorage);
        const token = await loadRpcToken(rpcTokenStorage);
        if (cancelled) return;
        if (saved) setRpcUrl(saved);
        if (token) setRpcToken(token);
      } catch {
        // A corrupt saved URL stays off the wire; the env default remains.
      } finally {
        if (!cancelled) setRpcHydrated(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    void loadSealedVault(vaultStorage).then((sealed) => {
      setVaultHasBlob(sealed !== null);
    });
  }, []);

  useEffect(() => {
    let cancelled = false;
    async function pollNode() {
      try {
        const info = await client.getNodeInfo();
        if (!cancelled) setNodeInfo(info);
      } catch {
        // Retain last successful nodeInfo / network across transient failures.
      }
    }
    if (!rpcHydrated) return;
    void pollNode();
    const id = window.setInterval(pollNode, Math.max(pollMs, 4000));
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [client, rpcHydrated]);

  useEffect(() => {
    let cancelled = false;
    void client
      .getProtocolTreasuries()
      .then((rows) => {
        if (!cancelled) setChainTreasuries(rows);
      })
      .catch(() => {
        if (!cancelled) setChainTreasuries(null);
      });
    return () => {
      cancelled = true;
    };
  }, [client]);

  useEffect(() => {
    if (!lastTxId) {
      setTxLookup(null);
      return;
    }
    return watchTransaction({
      client,
      txId: lastTxId,
      pollMs,
      onUpdate: setTxLookup,
    });
  }, [client, lastTxId]);

  // Gate HRP-sensitive wallet work until the first successful node info.
  const walletNetwork = nodeInfo
    ? walletNetworkFromNode(nodeInfo.network)
    : null;
  const networkReady = walletNetwork !== null;
  const netLabel = nodeInfo ? networkLabel(nodeInfo.network) : null;
  const netAccent = networkAccent(nodeInfo?.network);

  useEffect(() => {
    if (!networkReady || !mnemonic.trim()) return;
    try {
      const bech32 = addressBech32FromMnemonic(mnemonic, 0, "", walletNetwork);
      setReceiveBech32(bech32);
      setReceiveHex(parseAddress(bech32));
      setAddress(bech32);
    } catch {
      /* keep previous receive address on transient mnemonic edits */
    }
  }, [walletNetwork, networkReady, mnemonic]);

  const statusColor =
    snap.status === "ok"
      ? "var(--agora-cyan)"
      : snap.status === "error"
        ? "var(--agora-danger)"
        : "var(--agora-ink-muted)";

  async function onLookup(e: FormEvent) {
    e.preventDefault();
    let resolved: string;
    try {
      resolved = parseAddress(address, walletNetwork ?? undefined);
    } catch (err) {
      setWalletError(
        err instanceof Error
          ? err.message
          : "Enter a network-matching Bech32 or 40-character hex address",
      );
      setBalance(null);
      setNativeBalances(null);
      setUtxos([]);
      setChangeBalance(null);
      return;
    }
    setWalletBusy(true);
    setWalletError(null);
    try {
      const [bal, set, native] = await Promise.all([
        client.getBalance(resolved),
        client.getUtxos(resolved),
        client.getNativeBalances(resolved),
      ]);
      setBalance(bal.balance);
      setNativeBalances(readNativeBalances(native));
      setUtxos(set.utxos);
      if (watchWallet && resolved === watchWallet.tltReceiveHex) {
        const change = await client.getUtxos(watchWallet.tltChangeHex);
        setChangeBalance(change.utxos.reduce((sum, utxo) => sum + utxo.value, 0));
      } else {
        setChangeBalance(null);
      }
    } catch (err) {
      setBalance(null);
      setNativeBalances(null);
      setUtxos([]);
      setChangeBalance(null);
      setWalletError(err instanceof Error ? err.message : "lookup failed");
    } finally {
      setWalletBusy(false);
    }
  }

  function onDerive() {
    if (!walletNetwork) {
      setSendError("Waiting for node network…");
      return;
    }
    try {
      const bech32 = addressBech32FromMnemonic(mnemonic, 0, "", walletNetwork);
      const hex = parseAddress(bech32);
      setReceiveBech32(bech32);
      setReceiveHex(hex);
      setAddress(bech32);
      setSendError(null);
      setCopyHint(null);
    } catch (err) {
      setReceiveBech32(null);
      setReceiveHex(null);
      setSendError(err instanceof Error ? err.message : "invalid mnemonic");
    }
  }

  function onGenerate() {
    if (!walletNetwork) {
      setSendError("Waiting for node network…");
      return;
    }
    const phrase = generateMnemonic(128);
    setWatchWallet(null);
    setMnemonic(phrase);
    setMnemonicVisible(false);
    setVaultUnlocked(true);
    try {
      const bech32 = addressBech32FromMnemonic(phrase, 0, "", walletNetwork);
      const hex = parseAddress(bech32);
      setReceiveBech32(bech32);
      setReceiveHex(hex);
      setAddress(bech32);
      setSendError(null);
      setCopyHint(null);
      setVaultMsg("New mnemonic in memory — set a password and Save vault");
    } catch (err) {
      setReceiveBech32(null);
      setReceiveHex(null);
      setSendError(err instanceof Error ? err.message : "generate failed");
    }
  }

  async function onSaveVault() {
    setVaultBusy(true);
    setVaultMsg(null);
    try {
      const sealed = await sealVault(mnemonic, vaultPassword);
      await persistSealedVault(vaultStorage, sealed);
      setVaultHasBlob(true);
      setVaultUnlocked(true);
      setVaultMsg("Vault saved (AES-GCM). Lock to clear mnemonic from memory.");
    } catch (err) {
      setVaultMsg(err instanceof Error ? err.message : "save failed");
    } finally {
      setVaultBusy(false);
    }
  }

  async function onUnlockVault() {
    setVaultBusy(true);
    setVaultMsg(null);
    try {
      if (!walletNetwork) {
        throw new Error("Waiting for node network…");
      }
      const sealed = await loadSealedVault(vaultStorage);
      if (!sealed) {
        throw new Error("no vault on this device");
      }
      const phrase = await openVault(sealed, vaultPassword);
      setWatchWallet(null);
      setMnemonic(phrase);
      setMnemonicVisible(false);
      setVaultUnlocked(true);
      const bech32 = addressBech32FromMnemonic(phrase, 0, "", walletNetwork);
      setReceiveBech32(bech32);
      setReceiveHex(parseAddress(bech32));
      setAddress(bech32);
      setVaultMsg("Vault unlocked");
    } catch (err) {
      setVaultMsg(err instanceof Error ? err.message : "unlock failed");
    } finally {
      setVaultBusy(false);
    }
  }

  function onLockVault() {
    setMnemonic("");
    setVaultUnlocked(false);
    setVaultPassword("");
    setVaultMsg("Locked — mnemonic cleared from memory");
  }

  async function onDeleteVault() {
    setVaultBusy(true);
    try {
      await clearPersistedVault(vaultStorage);
      setVaultHasBlob(false);
      setMnemonic("");
      setVaultUnlocked(false);
      setVaultPassword("");
      setVaultMsg("Persisted vault deleted");
    } finally {
      setVaultBusy(false);
    }
  }

  async function onCopyReceive() {
    if (!receiveBech32 || !networkReady) return;
    const ok = await copyText(receiveBech32);
    setCopyHint(ok ? "Copied Bech32 address" : "Clipboard unavailable");
  }

  function onImportWatch(watch: WatchOnlyWallet) {
    setWatchWallet(watch);
    setMnemonic("");
    setVaultUnlocked(false);
    setReceiveBech32(watch.tltReceive);
    setReceiveHex(watch.tltReceiveHex);
    setAddress(watch.tltReceive);
    setSendError(null);
    setVaultMsg("Watch-only. This device cannot spend. The sealed vault, if any, is unchanged.");
  }

  function onImportRestore(phrase: string) {
    setWatchWallet(null);
    setMnemonic(phrase);
    setMnemonicVisible(false);
    setVaultUnlocked(true);
    setVaultMsg(
      "Mnemonic restored in memory and hidden. Anyone with these words can spend. Save the vault on this device.",
    );
    if (!walletNetwork) return;
    try {
      const bech32 = addressBech32FromMnemonic(phrase, 0, "", walletNetwork);
      setReceiveBech32(bech32);
      setReceiveHex(parseAddress(bech32));
      setAddress(bech32);
    } catch (err) {
      setSendError(err instanceof Error ? err.message : "invalid mnemonic");
    }
  }

  async function onSaveRpc(url: string, token: string) {
    const saved = await saveRpcEndpoint(rpcStorage, url);
    await saveRpcToken(rpcTokenStorage, token);
    setRpcUrl(saved);
    setRpcToken(token.trim());
    setNodeInfo(null);
  }

  async function onSend(e: FormEvent) {
    e.preventDefault();
    if (watchWallet) {
      try {
        await signSpend(watchWallet, async () => {
          throw new Error("unreachable");
        });
      } catch (err) {
        setSendError(
          err instanceof Error ? err.message : "watch-only wallet cannot sign or spend",
        );
      }
      return;
    }
    if (!walletNetwork) {
      setSendError("Waiting for node network…");
      return;
    }
    const amt = Number(amount);
    const feeN = Number(fee);
    if (!Number.isFinite(amt) || amt <= 0) {
      setSendError("Amount must be a positive number");
      return;
    }
    if (!Number.isFinite(feeN) || feeN < 1) {
      setSendError("Fee must be ≥ 1 (min relay)");
      return;
    }
    setSendBusy(true);
    setSendError(null);
    setLastTxId(null);
    setTxLookup(null);
    try {
      const { tx_id, built } = await sendTransfer(client, {
        mnemonic,
        toAddressHex: toAddress.trim(),
        amount: Math.floor(amt),
        fee: Math.floor(feeN),
        network: walletNetwork,
      });
      setLastTxId(tx_id);
      setReceiveBech32(built.fromBech32);
      setReceiveHex(built.from);
      setAddress(built.fromBech32);
      const [bal, set] = await Promise.all([
        client.getBalance(built.from),
        client.getUtxos(built.from),
      ]);
      setBalance(bal.balance);
      setUtxos(set.utxos);
    } catch (err) {
      setSendError(err instanceof Error ? err.message : "send failed");
    } finally {
      setSendBusy(false);
    }
  }

  const fieldStyle = {
    width: "100%" as const,
    padding: "0.65rem 0.75rem",
    border: "1px solid color-mix(in srgb, var(--agora-gold) 35%, transparent)",
    background: "color-mix(in srgb, var(--agora-obsidian) 55%, transparent)",
    color: "var(--agora-ink)",
    fontFamily: "ui-monospace, monospace",
    fontSize: "0.85rem",
  };

  const btnStyle = {
    padding: "0.65rem 1rem",
    border: "1px solid var(--agora-gold)",
    background: "transparent",
    color: "var(--agora-gold)",
    cursor: "pointer" as const,
    fontFamily: "var(--agora-display)",
    letterSpacing: "0.04em",
  };

  const show = (...lanes: DesktopLane[]) => lanes.includes(lane);
  const communityLane = [
    "HOME",
    "DRC",
    "OVL",
    "TLT",
    "SWAP",
    "PASSPORT",
    "COMMUNITY",
    "MISSIONS",
    "ACADEMY",
    "GRANTS",
    "BOUNTIES",
    "GUILDS",
    "MERCHANTS",
    "EVENTS",
    "ASSEMBLY",
    "TREASURY",
    "SETTINGS",
  ].includes(lane);

  return (
    <div className="agora-frame">
      <nav className="agora-nav" aria-label="Primary">
        {DESKTOP_LANES.map((item) => (
          <button
            key={item}
            type="button"
            className={item === lane ? "is-active" : undefined}
            onClick={() => setLane(item)}
          >
            {item}
          </button>
        ))}
      </nav>
    <main className="agora-shell">
      {show("HOME", "ACTIVITY") ? (
      <>
      <img
        src="/agora-network.png"
        alt="Agora Network"
        className="agora-icon-lg agora-rise"
      />
      <h1
        className="agora-brand agora-rise agora-rise-delay-1"
        style={{ fontSize: "2.75rem", marginTop: "1.25rem" }}
      >
        Agora Network
      </h1>
      <p
        className="agora-net-badge agora-rise agora-rise-delay-1"
        style={{ color: netAccent, borderColor: netAccent }}
        aria-live="polite"
        title={
          nodeInfo
            ? `Connected node reports network=${nodeInfo.network}`
            : "Waiting for agora_getNodeInfo"
        }
      >
        <span
          className="agora-net-dot"
          style={{ background: netAccent }}
          aria-hidden
        />
        {netLabel ?? "Connecting…"}
        {nodeInfo ? (
          <span className="agora-net-hrp">
            {networkHrpHint(nodeInfo.network)}
          </span>
        ) : null}
      </p>
      <p
        className="agora-lede agora-rise agora-rise-delay-2"
        style={{ marginTop: "0.85rem" }}
      >
        Desktop wallet for the whole Agora Network. Pair a phone with a watch-only
        code or a reveal-once mnemonic restore. Balances and header checks then
        run on each device.
      </p>

      <section
        className="agora-rise agora-rise-delay-3"
        style={{ marginTop: "2.5rem", maxWidth: 520 }}
        aria-live="polite"
      >
        <p className="agora-eyebrow">Node</p>
        <p style={{ marginTop: "0.65rem", fontSize: "0.95rem" }}>
          <span style={{ color: statusColor }}>
            {snap.status === "ok"
              ? "synced"
              : snap.status === "error"
                ? "offline"
                : "connecting"}
          </span>
          {" · "}
          {snap.tips.length} tip{snap.tips.length === 1 ? "" : "s"}
          {nodeInfo ? (
            <>
              {" · "}
              mempool {nodeInfo.mempool_count}
              {" · "}
              {nodeInfo.pow_algorithm}
              {" · "}
              {nodeInfo.archival ? "archival" : `hot ${nodeInfo.hot_window}`}
              {nodeInfo.connected_peers != null
                ? ` · peers ${nodeInfo.connected_peers}`
                : null}
              {nodeInfo.genesis_hash
                ? ` · genesis ${nodeInfo.genesis_hash.slice(0, 10)}…`
                : null}
            </>
          ) : null}
        </p>
        <p
          style={{
            marginTop: "0.35rem",
            opacity: 0.75,
            fontFamily: "ui-monospace, monospace",
            fontSize: "0.8rem",
          }}
        >
          {client.rpcUrl}
          {nodeInfo?.version ? ` · ${nodeInfo.version}` : null}
        </p>
        {snap.error ? (
          <p style={{ marginTop: "0.5rem", color: "var(--agora-danger)", fontSize: "0.9rem" }}>
            {snap.error}
          </p>
        ) : null}
        <ul
          style={{
            marginTop: "1.25rem",
            padding: 0,
            listStyle: "none",
            display: "grid",
            gap: "0.65rem",
          }}
        >
          {snap.tips.slice(0, 8).map((tip) => {
            const block = snap.tipBlocks.find((b) => b.id === tip);
            return (
              <li
                key={tip}
                style={{
                  fontFamily: "ui-monospace, monospace",
                  fontSize: "0.85rem",
                  color: "var(--agora-ink)",
                }}
              >
                <span style={{ color: "var(--agora-cyan)" }}>tip</span>{" "}
                {shortHash(tip)}
                {block ? (
                  <span style={{ color: "var(--agora-ink-muted)" }}>
                    {" "}
                    · bits {block.header.bits} · parents{" "}
                    {block.header.parents.length}
                  </span>
                ) : null}
              </li>
            );
          })}
          {snap.status === "ok" && snap.tips.length === 0 ? (
            <li style={{ color: "var(--agora-ink-muted)" }}>No tips yet</li>
          ) : null}
        </ul>
        {snap.updatedAt ? (
          <p
            style={{
              marginTop: "1rem",
              fontSize: "0.75rem",
              color: "var(--agora-ink-muted)",
            }}
          >
            Updated {new Date(snap.updatedAt).toLocaleTimeString()} · poll{" "}
            {pollMs}ms
          </p>
        ) : null}
      </section>
      </>
      ) : (
        <p className="agora-eyebrow">{lane}</p>
      )}

      {show("HOME", "SETTINGS") ? (
      <PairingPanel
        network={nodeInfo?.network ?? null}
        genesisHash={nodeInfo?.genesis_hash ?? null}
        rpcUrl={rpcUrl}
        rpcToken={rpcToken}
        onSaveRpc={onSaveRpc}
        spendMnemonic={mnemonic}
        watchWallet={watchWallet}
        onImportWatch={onImportWatch}
        onImportRestore={onImportRestore}
        copyText={copyText}
      />
      ) : null}

      {show("TLT", "EXPLORER") ? (
      <LightClientPanel
        client={client}
        network={nodeInfo?.network ?? null}
        genesisHash={nodeInfo?.genesis_hash ?? null}
      />
      ) : null}
      {lane === "EXPLORER" ? (
        <p className="agora-meta">
          Header checks run in this light client. The web explorer remains a separate app.
          <span className="agora-source"> indexed</span>
        </p>
      ) : null}

      {show("HOME", "WALLET", "DRC", "OVL", "TLT") ? (
      <FeatureSurfacesPanel
        client={client}
        network={nodeInfo?.network ?? null}
        genesisHash={nodeInfo?.genesis_hash ?? null}
        chainId={nodeInfo?.chain_id ?? null}
        tltHex={receiveHex ?? watchWallet?.tltReceiveHex ?? null}
        drcHex={watchWallet?.drcAccount ?? receiveHex ?? null}
        ovlHex={watchWallet?.ovlAccount ?? receiveHex ?? null}
        utxos={utxos}
        spendMnemonic={mnemonic}
        watchWallet={watchWallet}
      />
      ) : null}

      {show("WALLET", "DRC", "OVL", "TLT") ? (
      <p className="agora-role-row">
        <span>DRC PAY · move value</span>
        <span>OVL BUILD · contracts</span>
        <span>TLT SECURE · PoW/UTXO</span>
      </p>
      ) : null}

      {show("HOME", "SETTINGS") ? <DocsAboutPanel /> : null}

      {show("WALLET") ? (
      <section
        className="agora-rise agora-rise-delay-3"
        style={{ marginTop: "2.75rem", maxWidth: 520 }}
      >
        <p className="agora-eyebrow">Receive</p>
        <p style={{ marginTop: "0.55rem", fontSize: "0.85rem", color: "var(--agora-ink-muted)" }}>
          {networkReady
            ? `Primary address is Bech32m (${networkHrpHint(walletNetwork)}). Hex is shown for tooling.`
            : "Waiting for node network before deriving a receive address."}
        </p>
        {receiveBech32 && networkReady ? (
          <div style={{ marginTop: "0.85rem" }}>
            <p
              style={{
                fontFamily: "ui-monospace, monospace",
                fontSize: "0.85rem",
                color: "var(--agora-cyan)",
                wordBreak: "break-all",
              }}
            >
              {receiveBech32}
            </p>
            {receiveHex ? (
              <p
                style={{
                  marginTop: "0.35rem",
                  fontFamily: "ui-monospace, monospace",
                  fontSize: "0.75rem",
                  color: "var(--agora-ink-muted)",
                  wordBreak: "break-all",
                }}
              >
                hex {receiveHex}
              </p>
            ) : null}
            <div style={{ marginTop: "0.65rem", display: "flex", gap: "0.75rem", alignItems: "center" }}>
              <button
                type="button"
                onClick={() => void onCopyReceive()}
                disabled={!networkReady}
                style={btnStyle}
              >
                Copy address
              </button>
              {copyHint ? (
                <span style={{ fontSize: "0.8rem", color: "var(--agora-ink-muted)" }}>
                  {copyHint}
                </span>
              ) : null}
            </div>
          </div>
        ) : (
          <p style={{ marginTop: "0.85rem", fontSize: "0.9rem", color: "var(--agora-ink-muted)" }}>
            Generate or derive a mnemonic below to get a receive address.
          </p>
        )}
      </section>
      ) : null}

      {show("WALLET", "DRC", "OVL", "TLT") ? (
      <section
        className="agora-rise agora-rise-delay-3"
        style={{ marginTop: "2.75rem", maxWidth: 520 }}
      >
        <p className="agora-eyebrow">Wallet</p>
        <form
          onSubmit={onLookup}
          style={{
            marginTop: "0.85rem",
            display: "grid",
            gap: "0.75rem",
            gridTemplateColumns: "1fr auto",
            alignItems: "center",
          }}
        >
          <input
            value={address}
            onChange={(e) => setAddress(e.target.value)}
            placeholder={
              networkReady
                ? `${networkHrpHint(walletNetwork)} or 40-hex`
                : "40-hex (or wait for network)"
            }
            aria-label="Address"
            spellCheck={false}
            style={fieldStyle}
          />
          <button type="submit" disabled={walletBusy} style={btnStyle}>
            {walletBusy ? "…" : "Lookup"}
          </button>
        </form>
        {walletError ? (
          <p style={{ marginTop: "0.65rem", color: "var(--agora-danger)", fontSize: "0.9rem" }}>
            {walletError}
          </p>
        ) : null}
        {balance !== null ? (
          <div style={{ marginTop: "0.85rem", fontSize: "0.95rem" }}>
            <p>
              TLT{" "}
              <span style={{ color: "var(--agora-cyan)", fontFamily: "ui-monospace, monospace" }}>
                {nativeBalances?.assets.TLT.balance ?? balance}
              </span>{" "}
              base units · UTXO · node-reported · {utxos.length} output
              {utxos.length === 1 ? "" : "s"}
            </p>
            {nativeBalances ? (
              <>
                <p>
                  OVL{" "}
                  <span style={{ color: "var(--agora-gold)", fontFamily: "ui-monospace, monospace" }}>
                    {nativeBalances.assets.OVL.balance}
                  </span>{" "}
                  · account · node-reported
                </p>
                <p>
                  DRC{" "}
                  <span style={{ color: "var(--agora-cyan)", fontFamily: "ui-monospace, monospace" }}>
                    {nativeBalances.assets.DRC.balance}
                  </span>{" "}
                  · account · node-reported
                </p>
              </>
            ) : null}
            {changeBalance !== null ? (
              <p>
                TLT change chain{" "}
                <span style={{ color: "var(--agora-cyan)", fontFamily: "ui-monospace, monospace" }}>
                  {changeBalance}
                </span>{" "}
                · watch-only · node-reported
              </p>
            ) : null}
          </div>
        ) : null}
        {utxos.length > 0 ? (
          <ul
            style={{
              marginTop: "0.85rem",
              padding: 0,
              listStyle: "none",
              display: "grid",
              gap: "0.45rem",
            }}
          >
            {utxos.slice(0, 8).map((u) => (
              <li
                key={`${u.tx_id}:${u.index}`}
                style={{
                  fontFamily: "ui-monospace, monospace",
                  fontSize: "0.8rem",
                  color: "var(--agora-ink-muted)",
                }}
              >
                {shortHash(u.tx_id)}:{u.index} · {u.value}
              </li>
            ))}
          </ul>
        ) : null}
      </section>
      ) : null}

      {show("WALLET", "TLT") ? (
      <section
        className="agora-rise agora-rise-delay-3"
        style={{ marginTop: "2.75rem", maxWidth: 520 }}
      >
        <p className="agora-eyebrow">Send</p>
        <p style={{ marginTop: "0.55rem", fontSize: "0.85rem", color: "var(--agora-ink-muted)" }}>
          BIP-39 → m/44&apos;/8888&apos;/0&apos;/0/0. Password vault seals the mnemonic with
          AES-256-GCM (localStorage). Fee to miner (min relay 1).
          {watchWallet ? " Watch-only cannot sign or spend." : ""}
        </p>
        <form
          onSubmit={onSend}
          style={{ marginTop: "0.85rem", display: "grid", gap: "0.75rem" }}
        >
          <input
            type="password"
            value={vaultPassword}
            onChange={(e) => setVaultPassword(e.target.value)}
            placeholder="vault password (min 8 chars)"
            aria-label="Vault password"
            autoComplete="current-password"
            style={fieldStyle}
          />
          <div style={{ display: "flex", flexWrap: "wrap", gap: "0.75rem" }}>
            <button
              type="button"
              onClick={() => void onUnlockVault()}
              disabled={vaultBusy || !vaultHasBlob || !networkReady}
              style={btnStyle}
            >
              Unlock
            </button>
            <button
              type="button"
              onClick={() => void onSaveVault()}
              disabled={vaultBusy || !mnemonic}
              style={btnStyle}
            >
              Save vault
            </button>
            <button type="button" onClick={onLockVault} disabled={!vaultUnlocked} style={btnStyle}>
              Lock
            </button>
            <button
              type="button"
              onClick={() => void onDeleteVault()}
              disabled={vaultBusy || !vaultHasBlob}
              style={btnStyle}
            >
              Delete vault
            </button>
          </div>
          {vaultMsg ? (
            <p style={{ margin: 0, fontSize: "0.85rem", color: "var(--agora-ink-muted)" }}>
              {vaultHasBlob ? "Vault on disk · " : ""}
              {vaultUnlocked ? "unlocked · " : "locked · "}
              {vaultMsg}
            </p>
          ) : (
            <p style={{ margin: 0, fontSize: "0.85rem", color: "var(--agora-ink-muted)" }}>
              {vaultHasBlob
                ? vaultUnlocked
                  ? "Vault unlocked in memory"
                  : "Vault found — unlock with password"
                : "No vault yet — generate or paste a mnemonic, then Save vault"}
            </p>
          )}
          <textarea
            value={mnemonicVisible ? mnemonic : ""}
            onChange={(e) => {
              if (!mnemonicVisible) return;
              setMnemonic(e.target.value);
              if (e.target.value.trim()) {
                setVaultUnlocked(true);
                setWatchWallet(null);
              }
            }}
            readOnly={!mnemonicVisible && mnemonic.length > 0}
            placeholder={
              mnemonic && !mnemonicVisible
                ? "Mnemonic hidden. Show words only if you need to read them."
                : vaultHasBlob && !vaultUnlocked
                  ? "unlock vault to load mnemonic"
                  : "twelve or twenty-four word mnemonic"
            }
            aria-label="Mnemonic"
            rows={3}
            spellCheck={false}
            style={{ ...fieldStyle, resize: "vertical" as const }}
          />
          <div style={{ display: "flex", flexWrap: "wrap", gap: "0.75rem" }}>
            <button
              type="button"
              onClick={onGenerate}
              disabled={!networkReady}
              style={btnStyle}
            >
              Generate mnemonic
            </button>
            <button
              type="button"
              onClick={onDerive}
              disabled={!networkReady}
              style={btnStyle}
            >
              Derive address
            </button>
            {mnemonic ? (
              <button
                type="button"
                onClick={() => setMnemonicVisible((visible) => !visible)}
                style={btnStyle}
              >
                {mnemonicVisible ? "Hide mnemonic" : "Show mnemonic"}
              </button>
            ) : null}
            {receiveBech32 && networkReady ? (
              <span
                style={{
                  fontFamily: "ui-monospace, monospace",
                  fontSize: "0.75rem",
                  color: "var(--agora-cyan)",
                  alignSelf: "center",
                }}
                title={receiveBech32}
              >
                {shortAddress(receiveBech32)}
              </span>
            ) : null}
          </div>
          <input
            value={toAddress}
            onChange={(e) => setToAddress(e.target.value)}
            placeholder={
              networkReady
                ? `to ${networkHrpHint(walletNetwork)} or 40-hex`
                : "waiting for node network…"
            }
            aria-label="To address"
            spellCheck={false}
            disabled={!networkReady}
            style={fieldStyle}
          />
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.75rem" }}>
            <input
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
              placeholder="amount"
              aria-label="Amount"
              inputMode="numeric"
              style={fieldStyle}
            />
            <input
              value={fee}
              onChange={(e) => setFee(e.target.value)}
              placeholder="fee"
              aria-label="Fee"
              inputMode="numeric"
              style={fieldStyle}
            />
          </div>
          <button
            type="submit"
            disabled={sendBusy || !networkReady || watchWallet !== null}
            style={{ ...btnStyle, cursor: sendBusy ? "wait" : "pointer" }}
          >
            {sendBusy ? "Signing…" : "Sign & send"}
          </button>
        </form>
        {sendError ? (
          <p style={{ marginTop: "0.65rem", color: "var(--agora-danger)", fontSize: "0.9rem" }}>
            {sendError}
          </p>
        ) : null}
        {lastTxId ? (
          <p
            style={{
              marginTop: "0.65rem",
              fontFamily: "ui-monospace, monospace",
              fontSize: "0.8rem",
              color: "var(--agora-cyan)",
            }}
          >
            {shortHash(lastTxId)}
            {" · "}
            {txLookup?.status ?? "pending"}
            {txLookup?.status === "confirmed" && txLookup.confirmations != null
              ? ` · ${txLookup.confirmations} conf`
              : null}
            {txLookup?.status === "confirmed" && txLookup.block_id
              ? ` @ ${shortHash(txLookup.block_id)}`
              : null}
            {txLookup?.fee != null ? ` · fee ${txLookup.fee}` : null}
          </p>
        ) : null}
      </section>
      ) : null}
      {lane === "ACTIVITY" && !lastTxId ? (
        <p className="agora-meta">No signed broadcast yet. A cached screen is not a confirmation.</p>
      ) : null}

      {show("ASSEMBLY") ? (
      <GovernancePanel
        client={client}
        voterAddress={receiveBech32 || (address ? address : null)}
        balance={balance}
      />
      ) : null}
      {communityLane ? (
        <CommunityScreens
          lane={lane}
          client={community}
          address={receiveBech32 || address || null}
          chainTreasuries={chainTreasuries}
          notifications={notifications}
          onNotifications={setNotifications}
        />
      ) : null}
      {show("COMMUNITY", "PASSPORT", "ASSEMBLY", "TREASURY", "SETTINGS") ? (
        <CommunityTrustPanel subjectAddress={receiveBech32} community={community} />
      ) : null}
      {lane === "SETTINGS" ? (
        <ArchitecturePanel
          nodeUrl={rpcUrl}
          unlocked={vaultUnlocked}
          onLock={onLockVault}
          timeoutMs={sessionTimeoutMs}
          onTimeoutMs={setSessionTimeoutMs}
        />
      ) : null}
    </main>
    </div>
  );
}
