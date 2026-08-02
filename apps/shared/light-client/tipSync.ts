import type { LightBlock, LightClient, RpcStatus } from "./rpc";

export type TipSyncSnapshot = {
  status: RpcStatus;
  tips: string[];
  /** Tip blocks only (not full parent closure). */
  tipBlocks: LightBlock[];
  error: string | null;
  updatedAt: number | null;
};

export type TipSyncOptions = {
  client: LightClient;
  /** Poll interval in ms (default 2000). */
  pollMs?: number;
  /** Max tip blocks to hydrate per tick (default 12). */
  maxTipBlocks?: number;
  onUpdate: (snap: TipSyncSnapshot) => void;
};

const idle: TipSyncSnapshot = {
  status: "idle",
  tips: [],
  tipBlocks: [],
  error: null,
  updatedAt: null,
};

/**
 * Poll `agora_getDagTips` (+ optional tip block hydrate) until stopped.
 * Works in browser and React Native (`fetch` + `setTimeout`).
 */
export function startTipSync(options: TipSyncOptions): () => void {
  const pollMs = options.pollMs ?? 2000;
  const maxTipBlocks = options.maxTipBlocks ?? 12;
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const emit = (snap: TipSyncSnapshot) => {
    if (!stopped) options.onUpdate(snap);
  };

  const tick = async () => {
    try {
      const tips = await options.client.getDagTips();
      const tipBlocks: LightBlock[] = [];
      for (const tip of tips.slice(0, maxTipBlocks)) {
        if (stopped) return;
        try {
          tipBlocks.push(await options.client.getBlock(tip));
        } catch {
          // Tip may race with reorg / missing body — keep hash-only.
        }
      }
      emit({
        status: "ok",
        tips,
        tipBlocks,
        error: null,
        updatedAt: Date.now(),
      });
    } catch (err) {
      emit({
        status: "error",
        tips: [],
        tipBlocks: [],
        error: err instanceof Error ? err.message : "RPC unreachable",
        updatedAt: Date.now(),
      });
    } finally {
      if (!stopped) {
        timer = setTimeout(() => {
          void tick();
        }, pollMs);
      }
    }
  };

  emit(idle);
  void tick();

  return () => {
    stopped = true;
    if (timer !== undefined) clearTimeout(timer);
  };
}

export function shortHash(hex: string): string {
  return hex.length <= 12 ? hex : `${hex.slice(0, 8)}…${hex.slice(-4)}`;
}
