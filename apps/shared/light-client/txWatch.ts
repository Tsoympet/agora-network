import type { LightClient, LightTxLookup } from "./rpc";

export type TxWatchOptions = {
  client: LightClient;
  txId: string;
  pollMs?: number;
  onUpdate: (lookup: LightTxLookup) => void;
  /** Stop polling once status is `confirmed` (default true). */
  stopWhenConfirmed?: boolean;
  /**
   * Keep polling until `confirmations >= minConfirmations`.
   * Implies waiting for confirmed status. Default `1` when `stopWhenConfirmed`.
   */
  minConfirmations?: number;
};

/**
 * Poll `agora_getTransaction` until confirmed (and optional confirmation depth).
 * Returns a dispose function.
 */
export function watchTransaction(options: TxWatchOptions): () => void {
  const pollMs = options.pollMs ?? 2000;
  const stopWhenConfirmed = options.stopWhenConfirmed ?? true;
  const minConfirmations = options.minConfirmations ?? (stopWhenConfirmed ? 1 : 0);
  let cancelled = false;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const done = (lookup: LightTxLookup): boolean => {
    if (!stopWhenConfirmed && minConfirmations <= 0) return false;
    if (lookup.status !== "confirmed") return false;
    const conf = lookup.confirmations ?? 0;
    return conf >= Math.max(1, minConfirmations);
  };

  const tick = async () => {
    try {
      const lookup = await options.client.getTransaction(options.txId);
      if (cancelled) return;
      options.onUpdate(lookup);
      if (done(lookup)) {
        return;
      }
    } catch {
      // Keep polling through transient RPC errors.
    }
    if (!cancelled) {
      timer = setTimeout(tick, pollMs);
    }
  };

  void tick();
  return () => {
    cancelled = true;
    if (timer !== undefined) clearTimeout(timer);
  };
}
