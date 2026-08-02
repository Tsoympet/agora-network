import { useEffect, useMemo, useState } from "react";
import {
  fetchLiveDag,
  rpcUrl,
  type ExplorerBlock,
  type RpcStatus,
} from "../lib/rpc";

type LaidOut = {
  id: string;
  x: number;
  y: number;
  isTip: boolean;
  bits: number;
  short: string;
};

function shortHash(hex: string): string {
  return hex.length <= 10 ? hex : `${hex.slice(0, 6)}…${hex.slice(-4)}`;
}

/** Deterministic layout: tips on the right, parents layered left. */
function layoutBlocks(blocks: ExplorerBlock[], tips: Set<string>): LaidOut[] {
  const byId = new Map(blocks.map((b) => [b.id, b]));
  const depth = new Map<string, number>();

  function depthOf(id: string, stack = new Set<string>()): number {
    if (depth.has(id)) return depth.get(id)!;
    if (stack.has(id)) return 0;
    stack.add(id);
    const block = byId.get(id);
    if (!block || block.header.parents.length === 0) {
      depth.set(id, 0);
      return 0;
    }
    const d =
      Math.max(
        ...block.header.parents.map((p) =>
          byId.has(p) ? depthOf(p, stack) : 0,
        ),
      ) + 1;
    depth.set(id, d);
    return d;
  }

  for (const b of blocks) depthOf(b.id);

  const columns = new Map<number, string[]>();
  for (const b of blocks) {
    const d = depth.get(b.id) ?? 0;
    const col = columns.get(d) ?? [];
    col.push(b.id);
    columns.set(d, col);
  }

  const maxDepth = Math.max(0, ...columns.keys());
  const width = 1000;
  const height = 420;
  const padX = 80;
  const padY = 48;
  const out: LaidOut[] = [];

  for (const [d, ids] of columns) {
    const x =
      maxDepth === 0
        ? width / 2
        : padX + ((width - padX * 2) * d) / Math.max(1, maxDepth);
    ids.forEach((id, i) => {
      const y =
        ids.length === 1
          ? height / 2
          : padY + ((height - padY * 2) * i) / Math.max(1, ids.length - 1);
      const block = byId.get(id)!;
      out.push({
        id,
        x,
        y,
        isTip: tips.has(id),
        bits: block.header.bits,
        short: shortHash(id),
      });
    });
  }
  return out;
}

const POLL_MS = Number(import.meta.env.VITE_AGORA_POLL_MS) || 2000;

export function LiveDag() {
  const [tips, setTips] = useState<string[]>([]);
  const [blocks, setBlocks] = useState<ExplorerBlock[]>([]);
  const [status, setStatus] = useState<RpcStatus>("idle");
  const [error, setError] = useState<string | null>(null);
  const [updatedAt, setUpdatedAt] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;

    const tick = async () => {
      try {
        const snap = await fetchLiveDag();
        if (cancelled) return;
        setTips(snap.tips);
        setBlocks(snap.blocks);
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

  const tipSet = useMemo(() => new Set(tips), [tips]);
  const nodes = useMemo(
    () => layoutBlocks(blocks, tipSet),
    [blocks, tipSet],
  );
  const nodeMap = useMemo(
    () => new Map(nodes.map((n) => [n.id, n])),
    [nodes],
  );

  const edges = useMemo(() => {
    const lines: { key: string; x1: number; y1: number; x2: number; y2: number }[] =
      [];
    for (const block of blocks) {
      const child = nodeMap.get(block.id);
      if (!child) continue;
      for (const parent of block.header.parents) {
        const p = nodeMap.get(parent);
        if (!p) continue;
        lines.push({
          key: `${parent}->${block.id}`,
          x1: p.x,
          y1: p.y,
          x2: child.x,
          y2: child.y,
        });
      }
    }
    return lines;
  }, [blocks, nodeMap]);

  return (
    <div className="relative">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="agora-eyebrow">Live DAG</p>
          <h2 className="agora-display mt-3 text-3xl md:text-4xl">
            Tips in motion
          </h2>
          <p className="agora-lede mt-4">
            Polled from the node RPC—tip hashes and one parent layer, redrawn as
            the dag breathes.
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
          {tips.length} tip{tips.length === 1 ? "" : "s"}
          {" · "}
          <span className="font-mono text-xs opacity-80">{rpcUrl()}</span>
        </p>
      </div>

      <div className="relative mt-10 overflow-hidden rounded-none">
        <div className="pointer-events-none absolute inset-0 agora-glow bg-[radial-gradient(ellipse_at_30%_20%,rgba(197,152,53,0.12),transparent_55%)]" />
        <svg
          className="relative h-[28rem] w-full agora-dag-drift"
          viewBox="0 0 1000 420"
          role="img"
          aria-label="Live BlockDAG tips"
        >
          <g stroke="#C59835" strokeOpacity="0.4" strokeWidth="1.25">
            {edges.map((e) => (
              <line
                key={e.key}
                x1={e.x1}
                y1={e.y1}
                x2={e.x2}
                y2={e.y2}
              />
            ))}
          </g>
          {nodes.map((n, i) => (
            <g key={n.id} className="agora-rise" style={{ animationDelay: `${i * 40}ms` }}>
              <circle
                cx={n.x}
                cy={n.y}
                r={n.isTip ? 12 : 8}
                fill={n.isTip ? "#06BBDF" : "#C59835"}
                fillOpacity={n.isTip ? 0.95 : 0.8}
              />
              <circle
                cx={n.x}
                cy={n.y}
                r={n.isTip ? 26 : 18}
                stroke={n.isTip ? "#06BBDF" : "#C59835"}
                strokeOpacity="0.28"
                fill="none"
              >
                {n.isTip ? (
                  <animate
                    attributeName="r"
                    values="22;28;22"
                    dur="2.4s"
                    repeatCount="indefinite"
                  />
                ) : null}
              </circle>
              <text
                x={n.x}
                y={n.y + 36}
                textAnchor="middle"
                fill="#e8e6e1"
                fontSize="12"
                fontFamily="ui-monospace, monospace"
                opacity="0.85"
              >
                {n.short}
              </text>
            </g>
          ))}
          {nodes.length === 0 ? (
            <text
              x="500"
              y="210"
              textAnchor="middle"
              fill="#9aa0ab"
              fontSize="16"
              fontFamily="var(--font-ui)"
            >
              {status === "error"
                ? error || "Waiting for agora-node RPC…"
                : "Listening for tips…"}
            </text>
          ) : null}
        </svg>
      </div>

      {tips.length > 0 ? (
        <ul className="mt-8 space-y-2 font-mono text-xs text-mist md:text-sm">
          {tips.slice(0, 8).map((tip) => (
            <li key={tip} className="truncate">
              <span className="text-[var(--agora-cyan)]">tip</span>{" "}
              <span className="text-[var(--agora-ink)]">{tip}</span>
            </li>
          ))}
        </ul>
      ) : null}

      {updatedAt ? (
        <p className="mt-4 text-xs text-mist">
          Updated {new Date(updatedAt).toLocaleTimeString()} · poll {POLL_MS}ms
        </p>
      ) : null}
    </div>
  );
}
