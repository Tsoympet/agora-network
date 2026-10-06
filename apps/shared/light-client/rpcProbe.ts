import type { LightClient } from "./rpc";

const PROBE_PARAMS: Record<string, unknown> = {
  agora_getLightHeaders: { limit: 1 },
  agora_getValidatorSet: { asset: "OVL" },
  agora_getNativeAssetSupply: { asset: "TLT" },
  agora_getUtxos: { address: "00".repeat(20) },
  agora_submitTransaction: { tx: null },
  agora_estimateFee: [],
  agora_getTransaction: { tx_id: "00".repeat(32) },
  agora_getTltInclusionProof: { tx_id: "00".repeat(32) },
  agora_getDrcAccountPolicy: { account: "00".repeat(20) },
  agora_getDrcPayment: { payment_id: "00".repeat(32) },
  agora_getDrcDepositPreauth: {
    recipient: "00".repeat(20),
    authorized_source: "00".repeat(20),
  },
  agora_getDrcAccountKeys: { account: "00".repeat(20) },
  agora_getDrcAccountSignerList: { account: "00".repeat(20) },
  agora_getDrcTicket: { owner: "00".repeat(20), ticket_sequence: 1 },
  agora_getDrcEscrow: { escrow_id: "00".repeat(32) },
  agora_getDrcCheck: { check_id: "00".repeat(32) },
  agora_getDrcPaymentChannel: { channel_id: "00".repeat(32) },
  agora_getDrcTrustLine: {
    holder: "00".repeat(20),
    issuer: "00".repeat(20),
    currency: "USD",
  },
  agora_getDrcIssuedAssetPolicy: {
    issuer: "00".repeat(20),
    currency: "USD",
  },
  agora_getDrcAccountObjects: { account: "00".repeat(20), limit: 1 },
  agora_submitAccountTransfer: { account_transfer: null },
  agora_submitDrcPayment: { payment: null },
};

function methodMissing(message: string): boolean {
  const lower = message.toLowerCase();
  return (
    lower.includes("method not found") ||
    lower.includes("unknown method") ||
    lower.includes("not implemented") ||
    lower.includes("unsupported method")
  );
}

/** Best-effort: invalid params still mean the method exists on the node. */
export async function probeRpcMethod(
  client: LightClient,
  method: string,
): Promise<boolean> {
  const params = PROBE_PARAMS[method] ?? [];
  try {
    await client.call(method, params);
    return true;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    if (methodMissing(message)) return false;
    return true;
  }
}

export async function probeLightClientRpc(
  client: LightClient,
  methods: string[],
): Promise<Set<string>> {
  const available = new Set<string>();
  await Promise.all(
    methods.map(async (method) => {
      if (await probeRpcMethod(client, method)) available.add(method);
    }),
  );
  return available;
}

export function rpcMethodsFromMatrix(
  rows: { rpcMethod?: string }[],
): string[] {
  const out = new Set<string>();
  for (const row of rows) {
    if (row.rpcMethod) out.add(row.rpcMethod);
  }
  return [...out];
}
