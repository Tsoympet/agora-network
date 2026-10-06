export { encodeAgoraQr, parseAgoraQr, assertPaymentPreview } from "../community/qr";
export { openMerchantSeal, sealMerchantPay, type OpenedMerchantPay } from "./seal";

export const MERCHANT_ACTIVITY = {
  status: "PLANNED" as const,
  fabricated: false as const,
  records: [] as const,
};
