/**
 * Builds a real DrcPaymentTx through the light-client envelope and submits it
 * only when the node exposes agora_submitDrcPayment. Confirmation stays false
 * until agora_getDrcPayment returns a receipt object.
 */
import { parseAddress } from "../light-client/address.ts";
import { submitDrcNativePayment } from "../light-client/featureSurfaceActions.ts";
import type { LightClient } from "../light-client/rpc.ts";
import { probeRpcMethod } from "../light-client/rpcProbe.ts";
import { applyDrcNodeResult, type DrcPayReceipt } from "./pay.ts";
import type { QrPreview } from "./qr.ts";
import type { WalletMode } from "./session.ts";

export async function submitInspectedDrcPayment(input: {
  mode: WalletMode;
  preview: QrPreview;
  mnemonic: string;
  network: string;
  genesisHex: string;
  chainId: string;
  client: LightClient;
  fee: bigint;
  nonce: bigint;
}): Promise<DrcPayReceipt> {
  const toHex = parseAddress(input.preview.destination ?? "", input.network);
  const supported = await probeRpcMethod(input.client, "agora_submitDrcPayment");
  if (!supported) {
    return applyDrcNodeResult({
      mode: input.mode,
      preview: input.preview,
      signed: true,
      rpcSupported: false,
      paymentId: null,
      nodeReceipt: null,
    });
  }
  let paymentId: string | null = null;
  try {
    paymentId = await submitDrcNativePayment({
      client: input.client,
      mnemonic: input.mnemonic,
      network: input.network,
      genesisHex: input.genesisHex,
      chainId: input.chainId,
      toHex,
      amount: BigInt(input.preview.amount ?? "0"),
      fee: input.fee,
      nonce: input.nonce,
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : "submit failed";
    return {
      destination: input.preview.destination!,
      amount: input.preview.amount!,
      asset: "DRC",
      signedLocally: true,
      broadcast: "unavailable",
      confirmed: false,
      paymentId: null,
      note: `Node rejected the DRC payment (${message}). Nothing is confirmed.`,
    };
  }
  if (!paymentId || paymentId === "submitted") {
    return applyDrcNodeResult({
      mode: input.mode,
      preview: input.preview,
      signed: true,
      rpcSupported: true,
      paymentId: paymentId && paymentId !== "submitted" ? paymentId : null,
      nodeReceipt: null,
    });
  }
  let nodeReceipt: unknown = null;
  try {
    const row = await input.client.getDrcPayment(paymentId);
    nodeReceipt = row.receipt ?? null;
  } catch {
    nodeReceipt = null;
  }
  return applyDrcNodeResult({
    mode: input.mode,
    preview: input.preview,
    signed: true,
    rpcSupported: true,
    paymentId,
    nodeReceipt,
  });
}
