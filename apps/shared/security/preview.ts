/**
 * A transfer preview is not a signature and not a broadcast.
 * Callers must pass the preview id back before any signer runs.
 */

import { sha256 } from "@noble/hashes/sha256";

import { bytesToHex } from "../core/hex";

export type UnsignedPreview = {
  id: string;
  asset: "DRC" | "OVL" | "TLT";
  to: string;
  amount: string;
  fee: string;
  warnings: readonly string[];
  signed: false;
  submitted: false;
};

export function previewNativeTransfer(input: {
  asset: "DRC" | "OVL" | "TLT";
  to: string;
  amount: string;
  fee: string;
}): UnsignedPreview {
  if (!/^[1-9][0-9]*$/.test(input.amount)) {
    throw new Error("amount must be a positive integer");
  }
  if (!/^[0-9]+$/.test(input.fee)) throw new Error("fee must be an integer");
  const to = input.to.trim();
  if (!to) throw new Error("destination required");
  const id = bytesToHex(
    sha256(new TextEncoder().encode([input.asset, to, input.amount, input.fee].join("|"))),
  );
  return {
    id,
    asset: input.asset,
    to,
    amount: input.amount,
    fee: input.fee,
    warnings: [
      "Nothing is signed until you confirm this preview.",
      "A preview is not a payment receipt.",
    ],
    signed: false,
    submitted: false,
  };
}

export async function signAfterPreview<T>(
  preview: UnsignedPreview,
  confirmation: { confirmed: true; id: string } | { confirmed: false },
  sign: () => Promise<T>,
): Promise<T> {
  if (!confirmation.confirmed || confirmation.id !== preview.id) {
    throw new Error("transaction preview was not confirmed");
  }
  return sign();
}
