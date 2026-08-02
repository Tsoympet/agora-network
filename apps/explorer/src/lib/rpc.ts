/** Thin JSON-RPC client for agora-node (browser / vite proxy). */

export type ExplorerBlock = {
  id: string;
  header: {
    version: number;
    parents: string[];
    timestamp_ms: number;
    bits: number;
    nonce: number;
    tx_root: string;
  };
  tx_count: number;
};

export type RpcStatus = "idle" | "ok" | "error";

const DEFAULT_RPC =
  (import.meta.env.VITE_AGORA_RPC_URL as string | undefined) || "/rpc";

let nextId = 1;

export function rpcUrl(): string {
  return DEFAULT_RPC;
}

export async function rpcCall<T>(
  method: string,
  params: unknown = [],
): Promise<T> {
  const res = await fetch(rpcUrl(), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ id: nextId++, method, params }),
  });
  if (!res.ok) {
    throw new Error(`RPC HTTP ${res.status}`);
  }
  const body = (await res.json()) as {
    result?: T;
    error?: { message?: string };
  };
  if (body.error) {
    throw new Error(body.error.message || "RPC error");
  }
  if (body.result === undefined) {
    throw new Error("RPC missing result");
  }
  return body.result;
}

export async function getDagTips(): Promise<string[]> {
  return rpcCall<string[]>("agora_getDagTips", []);
}

export async function getBlock(hash: string): Promise<ExplorerBlock> {
  return rpcCall<ExplorerBlock>("agora_getBlock", { hash });
}

/** Fetch tip blocks plus one parent layer for a compact live DAG view. */
export async function fetchLiveDag(maxNodes = 24): Promise<{
  tips: string[];
  blocks: ExplorerBlock[];
}> {
  const tips = await getDagTips();
  const seen = new Set<string>();
  const blocks: ExplorerBlock[] = [];

  for (const tip of tips) {
    if (seen.has(tip) || blocks.length >= maxNodes) continue;
    const block = await getBlock(tip);
    seen.add(block.id);
    blocks.push(block);
  }

  const parentIds = blocks.flatMap((b) => b.header.parents);
  for (const parent of parentIds) {
    if (seen.has(parent) || blocks.length >= maxNodes) continue;
    try {
      const block = await getBlock(parent);
      seen.add(block.id);
      blocks.push(block);
    } catch {
      // Parent may be pruned / unknown — skip.
    }
  }

  return { tips, blocks };
}
