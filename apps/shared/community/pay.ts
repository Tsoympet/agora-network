import { assertCommunitySpendAllowed, type WalletMode } from "./session.ts";
import type { QrPreview } from "./qr.ts";
import { assertPaymentPreview } from "./qr.ts";

export const DRC_PAY_STEPS = [
  "scan",
  "inspect",
  "verify-merchant",
  "confirm",
  "sign",
  "broadcast",
  "receipt",
] as const;

export type DrcPayStep = (typeof DRC_PAY_STEPS)[number];

export type MerchantCheck = {
  listed: boolean;
  displayName: string | null;
  source: "community submitted" | "indexed";
  holdsMerchantKeys: false;
};

export type DrcPayReceipt = {
  destination: string;
  amount: string;
  asset: "DRC";
  signedLocally: boolean;
  broadcast: "submitted" | "unavailable";
  /** True only when a node receipt object was returned. Never inferred. */
  confirmed: boolean;
  paymentId: string | null;
  note: string;
};

const ORDER = DRC_PAY_STEPS;

/** Each step advances by one. Skipping the confirm screen is rejected. */
export function advanceDrcPay(step: DrcPayStep): DrcPayStep {
  const index = ORDER.indexOf(step);
  if (index < 0 || index === ORDER.length - 1) {
    throw new Error(`cannot advance DRC pay from ${step}`);
  }
  return ORDER[index + 1]!;
}

export function assertConfirmShown(preview: QrPreview): void {
  assertPaymentPreview(preview);
}

export function signDrcPayIntent(input: {
  mode: WalletMode;
  preview: QrPreview;
  merchant: MerchantCheck;
  confirmed: boolean;
}): { destination: string; amount: string; signatureDomain: string } {
  if (!input.confirmed) throw new Error("confirm destination and amount before signing");
  assertCommunitySpendAllowed(input.mode);
  assertPaymentPreview(input.preview);
  if (input.merchant.holdsMerchantKeys) {
    throw new Error("merchant keys must not be present on this device");
  }
  return {
    destination: input.preview.destination!,
    amount: input.preview.amount!,
    signatureDomain: "agora-community-drc-pay-intent-v1",
  };
}

/**
 * Consensus DRC payment bytes are versioned in the node. This light client
 * does not reconstruct them, so broadcast fails closed instead of submitting
 * a look-alike transaction.
 */
export function broadcastDrcPay(input: {
  mode: WalletMode;
  preview: QrPreview;
  signed: boolean;
}): DrcPayReceipt {
  assertCommunitySpendAllowed(input.mode);
  assertPaymentPreview(input.preview);
  if (!input.signed) throw new Error("sign before broadcast");
  return {
    destination: input.preview.destination!,
    amount: input.preview.amount!,
    asset: "DRC",
    signedLocally: true,
    broadcast: "unavailable",
    confirmed: false,
    paymentId: null,
    note: "Signed on this device. The node did not accept agora_submitDrcPayment, so nothing was submitted and the payment is not confirmed.",
  };
}

/**
 * A node receipt is the only confirmation this client will show.
 * A payment id without a receipt stays unconfirmed. Absence of RPC support
 * does not invent a submission.
 */
export function applyDrcNodeResult(input: {
  mode: WalletMode;
  preview: QrPreview;
  signed: boolean;
  rpcSupported: boolean;
  paymentId: string | null;
  nodeReceipt: unknown;
}): DrcPayReceipt {
  assertCommunitySpendAllowed(input.mode);
  assertPaymentPreview(input.preview);
  if (!input.signed) throw new Error("sign before broadcast");
  const base = {
    destination: input.preview.destination!,
    amount: input.preview.amount!,
    asset: "DRC" as const,
    signedLocally: true,
  };
  if (!input.rpcSupported || !input.paymentId) {
    return {
      ...base,
      broadcast: "unavailable",
      confirmed: false,
      paymentId: null,
      note: "agora_submitDrcPayment is not available on this node, so nothing was submitted and the payment is not confirmed.",
    };
  }
  const hasReceipt =
    input.nodeReceipt !== null &&
    input.nodeReceipt !== undefined &&
    typeof input.nodeReceipt === "object";
  if (!hasReceipt) {
    return {
      ...base,
      broadcast: "submitted",
      confirmed: false,
      paymentId: input.paymentId,
      note: `Submitted as ${input.paymentId}. Unconfirmed until the node returns a DRC payment receipt. This client does not treat a payment id as confirmation.`,
    };
  }
  return {
    ...base,
    broadcast: "submitted",
    confirmed: true,
    paymentId: input.paymentId,
    note: `Node receipt present for ${input.paymentId}. This light client did not recompute a DRC header proof.`,
  };
}
