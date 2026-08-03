/** Shared HTTP JSON-RPC helpers for Agora light clients (desktop / mobile / explorer). */

export type LightTxOut = {
  value: number;
  address: string;
};

export type LightTxIn = {
  tx_id: string;
  index: number;
};

export type LightTx = {
  tx_id: string;
  version: number;
  inputs: LightTxIn[];
  outputs: LightTxOut[];
  nonce: number;
  /** True when the transaction has no inputs (coinbase / mint). */
  is_coinbase: boolean;
};

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
  transactions?: LightTx[];
};

export type LightUtxo = {
  tx_id: string;
  index: number;
  value: number;
};

export type LightBalance = {
  address: string;
  balance: number;
};

export type LightUtxoSet = {
  address: string;
  utxos: LightUtxo[];
};

export type SubmitTxResult = {
  tx_id: string;
};

export type LightTxStatus = "pending" | "confirmed" | "unknown";

export type LightTxLookup = {
  tx_id: string;
  status: LightTxStatus;
  block_id: string | null;
  index: number | null;
  fee: number | null;
  /** Blue-score / tip-depth confirmations when confirmed. */
  confirmations: number | null;
  transaction: LightTx | null;
};

export type LightMempoolEntry = {
  tx_id: string;
  fee: number | null;
  transaction: LightTx;
};

export type LightMempool = {
  count: number;
  transactions: LightMempoolEntry[];
};

export type LightNodeInfo = {
  network: string;
  version: string;
  peer_id: string | null;
  connected_peers: number | null;
  tip_count: number;
  mempool_count: number;
  pow_algorithm: string;
  bits: number;
  archival: boolean;
  hot_window: number;
  allow_fund: boolean;
  miner_address: string | null;
  /** Hex id of Block 0 when reported by the node. */
  genesis_hash: string | null;
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
  getTransaction: (txId: string) => Promise<LightTxLookup>;
  getMempool: (limit?: number) => Promise<LightMempool>;
  getNodeInfo: () => Promise<LightNodeInfo>;
  getBalance: (address: string) => Promise<LightBalance>;
  getUtxos: (address: string) => Promise<LightUtxoSet>;
  /** Submit a signed transaction JSON body (native serde / byte-array hashes). */
  submitTransaction: (tx: unknown) => Promise<SubmitTxResult>;
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
    getTransaction: (txId: string) =>
      call<LightTxLookup>("agora_getTransaction", { tx_id: txId }),
    getMempool: (limit = 128) =>
      call<LightMempool>("agora_getMempool", { limit }),
    getNodeInfo: () => call<LightNodeInfo>("agora_getNodeInfo", []),
    getBalance: (address: string) =>
      call<LightBalance>("agora_getBalance", { address }),
    getUtxos: (address: string) =>
      call<LightUtxoSet>("agora_getUtxos", { address }),
    submitTransaction: (tx: unknown) =>
      call<SubmitTxResult>("agora_submitTransaction", { tx }),
  };
}
