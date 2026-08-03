import { useEffect, useState } from "react";
import { getNodeInfo, rpcUrl, type LightNodeInfo, type RpcStatus } from "../lib/rpc";

const POLL_MS = Number(import.meta.env.VITE_AGORA_POLL_MS) || 2000;

export function NodeStatus() {
  const [info, setInfo] = useState<LightNodeInfo | null>(null);
  const [status, setStatus] = useState<RpcStatus>("idle");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;

    const tick = async () => {
      try {
        const next = await getNodeInfo();
        if (cancelled) return;
        setInfo(next);
        setStatus("ok");
        setError(null);
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
          <p className="agora-eyebrow">Node</p>
          <h2 className="agora-display mt-3 text-3xl md:text-4xl">Operator pulse</h2>
          <p className="agora-lede mt-4">
            Live{" "}
            <code className="text-[var(--agora-ink)]">agora_getNodeInfo</code> — tips,
            mempool, peers, PoW, and archival retention.
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
          <span className="font-mono text-xs opacity-80">{rpcUrl()}</span>
        </p>
      </div>

      {error ? (
        <p className="mt-6 text-sm text-[var(--agora-danger)]">{error}</p>
      ) : null}

      {info ? (
        <dl className="mt-10 grid gap-4 font-mono text-xs md:grid-cols-2 md:text-sm">
          <div>
            <dt className="text-mist">network / version</dt>
            <dd className="text-[var(--agora-ink)]">
              {info.network} · {info.version}
            </dd>
          </div>
          <div>
            <dt className="text-mist">genesis</dt>
            <dd
              className="truncate text-[var(--agora-ink)]"
              title={info.genesis_hash ?? undefined}
            >
              {info.genesis_hash
                ? `${info.genesis_hash.slice(0, 12)}…${info.genesis_hash.slice(-8)}`
                : "—"}
            </dd>
          </div>
          <div>
            <dt className="text-mist">pow</dt>
            <dd className="text-[var(--agora-ink)]">
              {info.pow_algorithm} · bits {info.bits}
            </dd>
          </div>
          <div>
            <dt className="text-mist">tips / mempool</dt>
            <dd className="text-[var(--agora-ink)]">
              {info.tip_count} tip{info.tip_count === 1 ? "" : "s"} ·{" "}
              {info.mempool_count} pending
            </dd>
          </div>
          <div>
            <dt className="text-mist">peers</dt>
            <dd className="text-[var(--agora-ink)]">
              {info.connected_peers ?? "—"} connected
              {info.peer_id ? (
                <span className="mt-1 block truncate text-mist" title={info.peer_id}>
                  {info.peer_id}
                </span>
              ) : null}
            </dd>
          </div>
          <div>
            <dt className="text-mist">storage</dt>
            <dd className="text-[var(--agora-ink)]">
              {info.archival ? "archival" : "pruned"} · hot window{" "}
              {info.hot_window === 0 ? "unlimited" : info.hot_window}
            </dd>
          </div>
          <div>
            <dt className="text-mist">miner</dt>
            <dd className="truncate text-[var(--agora-cyan)]" title={info.miner_address ?? ""}>
              {info.miner_address ?? "—"}
              {info.allow_fund ? (
                <span className="text-mist"> · fund enabled</span>
              ) : null}
            </dd>
          </div>
        </dl>
      ) : null}
    </div>
  );
}
