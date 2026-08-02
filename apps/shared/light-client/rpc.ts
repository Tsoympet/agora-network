/** Shared HTTP JSON-RPC helpers for Agora light clients (desktop / mobile / explorer). */

export type LightBlock = {
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

export type LightClientConfig = {
  /** Full URL or path to JSON-RPC (`http://127.0.0.1:8545/rpc` or `/rpc`). */
  rpcUrl: string;
};

export type LightClient = {
  rpcUrl: string;
  call: <T>(method: string, params?: unknown) => Promise<T>;
  getDagTips: () => Promise<string[]>;
  getBlock: (hash: string) => Promise<LightBlock>;
};

export function createLightClient(config: LightClientConfig): LightClient {
  let nextId = 1;
  const rpcUrl = config.rpcUrl;

  async function call<T>(method: string, params: unknown = []): Promise<T> {
    const res = await fetch(rpcUrl, {
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

  return {
    rpcUrl,
    call,
    getDagTips: () => call<string[]>("agora_getDagTips", []),
    getBlock: (hash: string) => call<LightBlock>("agora_getBlock", { hash }),
  };
}
