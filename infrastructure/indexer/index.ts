/**
 * Operator index. It forwards to a full node when one is configured and otherwise
 * returns no chain rows. It does not mint balances to fill a gap.
 */

export const INDEXER_TRUST =
  "Operator-run index. chainProof is always false. This process does not invent balances, UTXOs, or transactions. With no upstream full node it returns no chain rows.";

export type IndexerFetch = (
  input: string,
  init?: { method?: string; headers?: Record<string, string>; body?: string },
) => Promise<{ ok: boolean; status: number; json: () => Promise<unknown> }>;

export function createIndexer(options: { upstreamRpc?: string | null; fetchImpl?: IndexerFetch } = {}) {
  const upstreamRpc = options.upstreamRpc?.trim() || null;
  return {
    trust: INDEXER_TRUST,
    status() {
      return { upstream: upstreamRpc !== null, chainData: null };
    },
    async proxy(body: unknown): Promise<unknown> {
      if (!upstreamRpc) {
        const error = new Error("no upstream full node");
        (error as Error & { status: number }).status = 503;
        throw error;
      }
      const fetchImpl = options.fetchImpl ?? (globalThis.fetch as IndexerFetch);
      const response = await fetchImpl(upstreamRpc, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      });
      if (!response.ok) {
        throw new Error(`upstream HTTP ${response.status}`);
      }
      return response.json();
    },
  };
}

export type Indexer = ReturnType<typeof createIndexer>;
