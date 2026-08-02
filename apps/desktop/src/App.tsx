import { FormEvent, useEffect, useMemo, useState } from "react";
import {
  addressFromMnemonic,
  createLightClient,
  sendTransfer,
  shortHash,
  startTipSync,
  watchTransaction,
  type LightTxLookup,
  type LightUtxo,
  type TipSyncSnapshot,
} from "../../shared/light-client";

function resolveRpcUrl(): string {
  return (
    (import.meta.env.VITE_AGORA_RPC_URL as string | undefined) ||
    "http://127.0.0.1:8545/rpc"
  );
}

const pollMs = Number(import.meta.env.VITE_AGORA_POLL_MS) || 2000;

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
  const [address, setAddress] = useState("");
  const [balance, setBalance] = useState<number | null>(null);
  const [utxos, setUtxos] = useState<LightUtxo[]>([]);
  const [walletError, setWalletError] = useState<string | null>(null);
  const [walletBusy, setWalletBusy] = useState(false);

  const [mnemonic, setMnemonic] = useState("");
  const [toAddress, setToAddress] = useState("");
  const [amount, setAmount] = useState("1");
  const [fee, setFee] = useState("1");
  const [sendBusy, setSendBusy] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const [lastTxId, setLastTxId] = useState<string | null>(null);
  const [txLookup, setTxLookup] = useState<LightTxLookup | null>(null);
  const [derivedAddress, setDerivedAddress] = useState<string | null>(null);

  useEffect(() => startTipSync({ client, pollMs, onUpdate: setSnap }), [client]);

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

  const statusColor =
    snap.status === "ok"
      ? "var(--agora-cyan)"
      : snap.status === "error"
        ? "var(--agora-danger)"
        : "var(--agora-ink-muted)";

  async function onLookup(e: FormEvent) {
    e.preventDefault();
    const hex = address.trim().toLowerCase();
    if (!/^(0x)?[0-9a-f]{40}$/.test(hex)) {
      setWalletError("Enter a 40-character hex address");
      setBalance(null);
      setUtxos([]);
      return;
    }
    setWalletBusy(true);
    setWalletError(null);
    try {
      const [bal, set] = await Promise.all([
        client.getBalance(hex),
        client.getUtxos(hex),
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
      const hex = addressFromMnemonic(mnemonic);
      setDerivedAddress(hex);
      setAddress(hex);
      setSendError(null);
    } catch (err) {
      setDerivedAddress(null);
      setSendError(err instanceof Error ? err.message : "invalid mnemonic");
    }
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
      });
      setLastTxId(tx_id);
      setDerivedAddress(built.from);
      setAddress(built.from);
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
      <img src="/nexus-icon.svg" alt="" className="agora-icon-lg agora-rise" />
      <h1
        className="agora-brand agora-rise agora-rise-delay-1"
        style={{ fontSize: "2.75rem", marginTop: "1.25rem" }}
      >
        Agora Network
      </h1>
      <p
        className="agora-lede agora-rise agora-rise-delay-2"
        style={{ marginTop: "0.85rem" }}
      >
        Desktop wallet shell with RandomX sidecar hooks. Tip sync, UTXO lookup,
        and signed BIP-44 sends follow the live DAG over HTTP JSON-RPC.
      </p>

      <section
        className="agora-rise agora-rise-delay-3"
        style={{ marginTop: "2.5rem", maxWidth: 520 }}
        aria-live="polite"
      >
        <p className="agora-eyebrow">Light client</p>
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
          {" · "}
          <span style={{ opacity: 0.75, fontFamily: "ui-monospace, monospace", fontSize: "0.8rem" }}>
            {client.rpcUrl}
          </span>
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
            placeholder="40-hex address"
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
          BIP-39 mnemonic → m/44&apos;/8888&apos;/0&apos;/0/0. Fee paid to miner (min relay 1).
        </p>
        <form
          onSubmit={onSend}
          style={{ marginTop: "0.85rem", display: "grid", gap: "0.75rem" }}
        >
          <textarea
            value={mnemonic}
            onChange={(e) => setMnemonic(e.target.value)}
            placeholder="twelve or twenty-four word mnemonic"
            aria-label="Mnemonic"
            rows={3}
            style={{ ...fieldStyle, resize: "vertical" as const }}
          />
          <div style={{ display: "grid", gridTemplateColumns: "1fr auto", gap: "0.75rem" }}>
            <button type="button" onClick={onDerive} style={btnStyle}>
              Derive address
            </button>
            {derivedAddress ? (
              <span
                style={{
                  fontFamily: "ui-monospace, monospace",
                  fontSize: "0.75rem",
                  color: "var(--agora-cyan)",
                  alignSelf: "center",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                }}
              >
                {derivedAddress}
              </span>
            ) : null}
          </div>
          <input
            value={toAddress}
            onChange={(e) => setToAddress(e.target.value)}
            placeholder="to address (40-hex)"
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
