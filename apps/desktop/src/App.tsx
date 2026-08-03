import { FormEvent, useEffect, useMemo, useState } from "react";
import {
  addressBech32FromMnemonic,
  clearPersistedVault,
  createLightClient,
  generateMnemonic,
  loadSealedVault,
  localStorageVault,
  networkAccent,
  networkHrpHint,
  networkLabel,
  openVault,
  parseAddress,
  persistSealedVault,
  sealVault,
  sendTransfer,
  shortAddress,
  shortHash,
  startTipSync,
  walletNetworkFromNode,
  watchTransaction,
  type LightNodeInfo,
  type LightTxLookup,
  type LightUtxo,
  type TipSyncSnapshot,
} from "../../shared/light-client";

const vaultStorage = localStorageVault();

function resolveRpcUrl(): string {
  return (
    (import.meta.env.VITE_AGORA_RPC_URL as string | undefined) ||
    "http://127.0.0.1:8545/rpc"
  );
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
  const client = useMemo(
    () => createLightClient({ rpcUrl: resolveRpcUrl() }),
    [],
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
  const [utxos, setUtxos] = useState<LightUtxo[]>([]);
  const [walletError, setWalletError] = useState<string | null>(null);
  const [walletBusy, setWalletBusy] = useState(false);

  const [mnemonic, setMnemonic] = useState("");
  const [vaultPassword, setVaultPassword] = useState("");
  const [vaultHasBlob, setVaultHasBlob] = useState(false);
  const [vaultUnlocked, setVaultUnlocked] = useState(false);
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
  const [copyHint, setCopyHint] = useState<string | null>(null);

  useEffect(() => startTipSync({ client, pollMs, onUpdate: setSnap }), [client]);

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
        if (!cancelled) setNodeInfo(null);
      }
    }
    void pollNode();
    const id = window.setInterval(pollNode, Math.max(pollMs, 4000));
    return () => {
      cancelled = true;
      window.clearInterval(id);
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

  // Prefer live node network for Bech32 HRP; local default is devnet.
  const walletNetwork = walletNetworkFromNode(nodeInfo?.network ?? "devnet");
  const netLabel = nodeInfo ? networkLabel(nodeInfo.network) : null;
  const netAccent = networkAccent(nodeInfo?.network ?? "devnet");

  useEffect(() => {
    if (!mnemonic.trim()) return;
    try {
      const bech32 = addressBech32FromMnemonic(mnemonic, 0, "", walletNetwork);
      setReceiveBech32(bech32);
      setReceiveHex(parseAddress(bech32));
      setAddress(bech32);
    } catch {
      /* keep previous receive address on transient mnemonic edits */
    }
  }, [walletNetwork, mnemonic]);

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
      resolved = parseAddress(address);
    } catch {
      setWalletError("Enter an agora1… or 40-character hex address");
      setBalance(null);
      setUtxos([]);
      return;
    }
    setWalletBusy(true);
    setWalletError(null);
    try {
      const [bal, set] = await Promise.all([
        client.getBalance(resolved),
        client.getUtxos(resolved),
      ]);
      setBalance(bal.balance);
      setUtxos(set.utxos);
    } catch (err) {
      setBalance(null);
      setUtxos([]);
      setWalletError(err instanceof Error ? err.message : "lookup failed");
    } finally {
      setWalletBusy(false);
    }
  }

  function onDerive() {
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
    const phrase = generateMnemonic(128);
    setMnemonic(phrase);
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
      const sealed = await loadSealedVault(vaultStorage);
      if (!sealed) {
        throw new Error("no vault on this device");
      }
      const phrase = await openVault(sealed, vaultPassword);
      setMnemonic(phrase);
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
    if (!receiveBech32) return;
    const ok = await copyText(receiveBech32);
    setCopyHint(ok ? "Copied Bech32 address" : "Clipboard unavailable");
  }

  async function onSend(e: FormEvent) {
    e.preventDefault();
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

  return (
    <main className="agora-shell">
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
        <span className="agora-net-hrp">
          {networkHrpHint(nodeInfo?.network ?? "devnet")}
        </span>
      </p>
      <p
        className="agora-lede agora-rise agora-rise-delay-2"
        style={{ marginTop: "0.85rem" }}
      >
        Desktop wallet: Bech32 receive, BIP-39 send, and live DAG tip sync over
        HTTP JSON-RPC.
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

      <section
        className="agora-rise agora-rise-delay-3"
        style={{ marginTop: "2.75rem", maxWidth: 520 }}
      >
        <p className="agora-eyebrow">Receive</p>
        <p style={{ marginTop: "0.55rem", fontSize: "0.85rem", color: "var(--agora-ink-muted)" }}>
          Primary address is Bech32m (`agora1…`). Hex is shown for tooling.
        </p>
        {receiveBech32 ? (
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
              <button type="button" onClick={() => void onCopyReceive()} style={btnStyle}>
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
            placeholder="agora1… or 40-hex"
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
          <p style={{ marginTop: "0.85rem", fontSize: "0.95rem" }}>
            Balance{" "}
            <span style={{ color: "var(--agora-cyan)", fontFamily: "ui-monospace, monospace" }}>
              {balance}
            </span>{" "}
            base units · {utxos.length} UTXO{utxos.length === 1 ? "" : "s"}
          </p>
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

      <section
        className="agora-rise agora-rise-delay-3"
        style={{ marginTop: "2.75rem", maxWidth: 520 }}
      >
        <p className="agora-eyebrow">Send</p>
        <p style={{ marginTop: "0.55rem", fontSize: "0.85rem", color: "var(--agora-ink-muted)" }}>
          BIP-39 → m/44&apos;/8888&apos;/0&apos;/0/0. Password vault seals the mnemonic with
          AES-256-GCM (localStorage). Fee to miner (min relay 1).
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
              disabled={vaultBusy || !vaultHasBlob}
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
            value={mnemonic}
            onChange={(e) => {
              setMnemonic(e.target.value);
              if (e.target.value.trim()) setVaultUnlocked(true);
            }}
            placeholder={
              vaultHasBlob && !vaultUnlocked
                ? "unlock vault to load mnemonic"
                : "twelve or twenty-four word mnemonic"
            }
            aria-label="Mnemonic"
            rows={3}
            style={{ ...fieldStyle, resize: "vertical" as const }}
          />
          <div style={{ display: "flex", flexWrap: "wrap", gap: "0.75rem" }}>
            <button type="button" onClick={onGenerate} style={btnStyle}>
              Generate mnemonic
            </button>
            <button type="button" onClick={onDerive} style={btnStyle}>
              Derive address
            </button>
            {receiveBech32 ? (
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
            placeholder="to agora1… or 40-hex"
            aria-label="To address"
            spellCheck={false}
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
          <button type="submit" disabled={sendBusy} style={{ ...btnStyle, cursor: sendBusy ? "wait" : "pointer" }}>
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
    </main>
  );
}
