import type { LightClient, LightTxLookup } from "./rpc";

export type TxWatchOptions = {
  client: LightClient;
  txId: string;
  pollMs?: number;
  onUpdate: (lookup: LightTxLookup) => void;
  /** Stop polling once status is `confirmed` (default true). */
  stopWhenConfirmed?: boolean;
};

/**
 * Poll `agora_getTransaction` until confirmed (or the caller unsubscribes).
 * Returns a dispose function.
 */
export function watchTransaction(options: TxWatchOptions): () => void {
  const pollMs = options.pollMs ?? 2000;
  const stopWhenConfirmed = options.stopWhenConfirmed ?? true;
  let cancelled = false;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const tick = async () => {
    try {
      const lookup = await options.client.getTransaction(options.txId);
      if (cancelled) return;
      options.onUpdate(lookup);
      if (stopWhenConfirmed && lookup.status === "confirmed") {
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
