/** Explorer RPC façade over the shared light-client module. */

import {
  createLightClient,
  type LightBlock,
  type RpcStatus,
} from "../../../shared/light-client";

export type ExplorerBlock = LightBlock;
export type { RpcStatus };

const DEFAULT_RPC =
  (import.meta.env.VITE_AGORA_RPC_URL as string | undefined) || "/rpc";

const client = createLightClient({ rpcUrl: DEFAULT_RPC });

export function rpcUrl(): string {
  return client.rpcUrl;
}

export async function rpcCall<T>(
  method: string,
  params: unknown = [],
): Promise<T> {
  return client.call<T>(method, params);
}

export async function getDagTips(): Promise<string[]> {
  return client.getDagTips();
}

export async function getBlock(hash: string): Promise<ExplorerBlock> {
  return client.getBlock(hash);
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
