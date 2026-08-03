import { useEffect, useState } from "react";
import {
  getMempool,
  rpcUrl,
  type LightMempoolEntry,
  type RpcStatus,
} from "../lib/rpc";

const POLL_MS = Number(import.meta.env.VITE_AGORA_POLL_MS) || 2000;

function shortHash(h: string, n = 8): string {
  if (h.length <= n * 2) return h;
  return `${h.slice(0, n)}…${h.slice(-n)}`;
}

export function MempoolPanel() {
  const [entries, setEntries] = useState<LightMempoolEntry[]>([]);
  const [count, setCount] = useState(0);
  const [status, setStatus] = useState<RpcStatus>("idle");
  const [error, setError] = useState<string | null>(null);
  const [updatedAt, setUpdatedAt] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;

    const tick = async () => {
      try {
        const snap = await getMempool(64);
        if (cancelled) return;
        setEntries(snap.transactions);
        setCount(snap.count);
        setStatus("ok");
        setError(null);
        setUpdatedAt(Date.now());
      } catch (err) {
        if (cancelled) return;
        setStatus("error");
        setError(err instanceof Error ? err.message : "RPC unreachable");
      } finally {
        if (!cancelled) {
          timer = window.setTimeout(tick, POLL_MS);
        }
      }
    };

    void tick();
    return () => {
      cancelled = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, []);

  return (
    <div className="relative">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="agora-eyebrow">Mempool</p>
          <h2 className="agora-display mt-3 text-3xl md:text-4xl">
            Waiting for a tip
          </h2>
          <p className="agora-lede mt-4">
            Pending transfers from{" "}
            <code className="text-[var(--agora-ink)]">agora_getMempool</code>,
            ordered by fee. Click a row to open transaction lookup.
          </p>
        </div>
        <p className="text-sm text-mist">
          <span
            className={
              status === "ok"
                ? "text-[var(--agora-cyan)]"
                : status === "error"
                  ? "text-[var(--agora-danger)]"
                  : "text-mist"
            }
          >
            {status === "ok"
              ? "connected"
              : status === "error"
                ? "offline"
                : "connecting"}
          </span>
          {" · "}
          {count} pending
          {" · "}
          <span className="font-mono text-xs opacity-80">{rpcUrl()}</span>
        </p>
      </div>

      {error ? (
        <p className="mt-6 text-sm text-[var(--agora-danger)]">{error}</p>
      ) : null}

      <ul className="mt-10 space-y-4">
        {entries.length === 0 ? (
          <li className="text-sm text-mist">
            {status === "error"
              ? "Waiting for agora-node RPC…"
              : "Mempool is empty."}
          </li>
        ) : (
          entries.map((entry, i) => (
            <li
              key={entry.tx_id}
              className="agora-rise font-mono text-xs md:text-sm"
              style={{ animationDelay: `${i * 40}ms` }}
            >
              <a
                href="#tx"
                className="text-[var(--agora-cyan)] hover:underline"
                title={entry.tx_id}
                onClick={() => {
                  try {
                    sessionStorage.setItem("agora_explorer_tx", entry.tx_id);
                  } catch {
                    // ignore
                  }
                }}
              >
                {shortHash(entry.tx_id, 12)}
              </a>
              <span className="text-mist">
                {" · "}
                {entry.transaction.inputs.length} in ·{" "}
                {entry.transaction.outputs.length} out
                {entry.fee != null ? ` · fee ${entry.fee}` : ""}
              </span>
              <div className="mt-1 text-mist">
                {entry.transaction.outputs.slice(0, 3).map((o, oi) => (
                  <div key={`${entry.tx_id}-o${oi}`}>
                    → {shortHash(o.address)} · {o.value}
                  </div>
                ))}
              </div>
            </li>
          ))
        )}
      </ul>

      {updatedAt ? (
        <p className="mt-6 text-xs text-mist">
          Updated {new Date(updatedAt).toLocaleTimeString()} · poll {POLL_MS}ms
        </p>
      ) : null}
    </div>
  );
}
