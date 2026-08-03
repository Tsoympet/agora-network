import { FormEvent, useEffect, useRef, useState } from "react";
import {
  shortAddress,
  watchTransaction,
  type LightTxLookup,
} from "../../../shared/light-client";
import { getClient, getTransaction, rpcUrl } from "../lib/rpc";

const TX_HEX = /^(0x)?[0-9a-fA-F]{64}$/;
const POLL_MS = Number(import.meta.env.VITE_AGORA_POLL_MS) || 2000;

function shortHash(h: string, n = 8): string {
  if (h.length <= n * 2) return h;
  return `${h.slice(0, n)}…${h.slice(-n)}`;
}

function statusTone(status: LightTxLookup["status"]): string {
  switch (status) {
    case "confirmed":
      return "text-[var(--agora-cyan)]";
    case "pending":
      return "text-[var(--agora-gold)]";
    default:
      return "text-[var(--agora-danger)]";
  }
}

function seedTxId(): string | null {
  try {
    const fromLive = sessionStorage.getItem("agora_explorer_tx");
    if (fromLive) {
      sessionStorage.removeItem("agora_explorer_tx");
      if (TX_HEX.test(fromLive)) return fromLive;
    }
  } catch {
    // ignore storage failures
  }
  const q = new URLSearchParams(window.location.search).get("tx");
  return q && TX_HEX.test(q) ? q : null;
}

export function TxLookup() {
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<LightTxLookup | null>(null);
  const [watching, setWatching] = useState(false);
  const stopRef = useRef<(() => void) | null>(null);
  const seededRef = useRef(false);

  function stopWatch() {
    stopRef.current?.();
    stopRef.current = null;
    setWatching(false);
  }

  async function lookup(txId: string) {
    stopWatch();
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const found = await getTransaction(txId);
      setResult(found);
      if (found.status === "pending") {
        setWatching(true);
        stopRef.current = watchTransaction({
          client: getClient(),
          txId: found.tx_id,
          pollMs: POLL_MS,
          onUpdate: (next) => {
            setResult(next);
            if (next.status !== "pending") {
              stopWatch();
            }
          },
        });
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    return () => {
      stopRef.current?.();
      stopRef.current = null;
    };
  }, []);

  useEffect(() => {
    if (seededRef.current) return;
    seededRef.current = true;
    const seeded = seedTxId();
    if (!seeded) return;
    setInput(seeded);
    void lookup(seeded);
    // Mount-only seed from LiveDag / ?tx=
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function onSubmit(e: FormEvent) {
    e.preventDefault();
    const raw = input.trim();
    if (!TX_HEX.test(raw)) {
      setError("Enter a 64-hex transaction id (optional 0x prefix).");
      setResult(null);
      return;
    }
    void lookup(raw);
  }

  const outs = result?.transaction?.outputs ?? [];

  return (
    <div className="relative">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="agora-eyebrow">Transaction lookup</p>
          <h2 className="agora-display mt-3 text-3xl md:text-4xl">
            Trace a transfer
          </h2>
          <p className="agora-lede mt-4">
            Resolve a tx via{" "}
            <code className="text-[var(--agora-ink)]">agora_getTransaction</code>
            — pending in the mempool, confirmed on the dag, or unknown. Pending
            lookups keep polling until confirmation.
          </p>
        </div>
        <p className="font-mono text-xs text-mist opacity-80">{rpcUrl()}</p>
      </div>

      <form
        className="mt-10 flex flex-col gap-4 md:flex-row md:items-end"
        onSubmit={onSubmit}
      >
        <label className="flex min-w-0 flex-1 flex-col gap-2">
          <span className="text-sm text-mist">Transaction id</span>
          <input
            type="text"
            spellCheck={false}
            autoComplete="off"
            placeholder="64 hex characters"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            disabled={busy}
            className="w-full border border-[var(--agora-line)] bg-[rgba(14,16,20,0.55)] px-4 py-3 font-mono text-sm text-[var(--agora-ink)] outline-none focus:border-[var(--agora-gold)]"
          />
        </label>
        <div className="flex flex-wrap gap-3">
          <button
            type="submit"
            disabled={busy || !input.trim()}
            className="agora-btn agora-btn-primary disabled:opacity-50"
          >
            {busy ? "Looking up…" : "Lookup"}
          </button>
          {watching ? (
            <button
              type="button"
              className="agora-btn agora-btn-ghost"
              onClick={stopWatch}
            >
              Stop watching
            </button>
          ) : null}
        </div>
      </form>

      {error ? (
        <p className="mt-4 text-sm text-[var(--agora-danger)]">{error}</p>
      ) : null}

      {result ? (
        <div className="mt-10">
          <p className="agora-eyebrow">Result</p>
          <h3
            className={`agora-display mt-3 text-2xl ${statusTone(result.status)}`}
          >
            {result.status}
            {watching && result.status === "pending" ? (
              <span className="ml-2 text-base text-mist">watching…</span>
            ) : null}
          </h3>
          <dl className="mt-6 space-y-3 font-mono text-xs md:text-sm">
            <div className="flex flex-wrap gap-x-3 gap-y-1">
              <dt className="text-mist">tx</dt>
              <dd
                className="min-w-0 break-all text-[var(--agora-ink)]"
                title={result.tx_id}
              >
                {result.tx_id}
              </dd>
            </div>
            {result.fee != null ? (
              <div className="flex flex-wrap gap-x-3 gap-y-1">
                <dt className="text-mist">fee</dt>
                <dd className="text-[var(--agora-ink)]">{result.fee} gold</dd>
              </div>
            ) : null}
            {result.confirmations != null ? (
              <div className="flex flex-wrap gap-x-3 gap-y-1">
                <dt className="text-mist">confirmations</dt>
                <dd className="text-[var(--agora-ink)]">{result.confirmations}</dd>
              </div>
            ) : null}
            {result.block_id ? (
              <div className="flex flex-wrap gap-x-3 gap-y-1">
                <dt className="text-mist">block</dt>
                <dd>
                  <a
                    href="#live"
                    className="text-[var(--agora-cyan)] hover:underline"
                    title={result.block_id}
                  >
                    {shortHash(result.block_id, 12)}
                  </a>
                  {result.index != null ? (
                    <span className="text-mist"> · index {result.index}</span>
                  ) : null}
                </dd>
              </div>
            ) : null}
            {outs.length > 0 ? (
              <div>
                <dt className="text-mist">outputs</dt>
                <dd className="mt-2 space-y-2">
                  {outs.map((o, i) => (
                    <div
                      key={`${o.address}-${i}`}
                      className="text-[var(--agora-ink)]"
                    >
                      <span title={o.address}>{shortAddress(o.address)}</span>
                      <span className="text-mist"> → </span>
                      {o.value}
                      <span className="text-mist"> gold</span>
                    </div>
                  ))}
                </dd>
              </div>
            ) : null}
          </dl>
          {result.status === "unknown" ? (
            <p className="mt-4 text-sm text-mist">
              Not in the mempool and not indexed on a confirmed block yet (or
              pruned / never seen).
            </p>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
