import { useEffect, useMemo, useState } from "react";
import {
  createLightClient,
  shortHash,
  startTipSync,
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

  useEffect(() => startTipSync({ client, pollMs, onUpdate: setSnap }), [client]);

  const statusColor =
    snap.status === "ok"
      ? "var(--agora-cyan)"
      : snap.status === "error"
        ? "var(--agora-danger)"
        : "var(--agora-ink-muted)";

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
        Desktop wallet shell with RandomX sidecar hooks. Tip sync follows the
        live DAG over HTTP JSON-RPC.
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
    </main>
  );
}
