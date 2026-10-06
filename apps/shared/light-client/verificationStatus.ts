/** Honest verification labels for wallet surfaces (Obsidian & Gold UI). */

export type VerificationStatus = "verified-locally" | "node-reported" | "unavailable";

export function verificationLabel(status: VerificationStatus): string {
  switch (status) {
    case "verified-locally":
      return "verified locally";
    case "node-reported":
      return "node-reported";
    case "unavailable":
      return "unavailable";
  }
}

export function verificationShort(status: VerificationStatus): string {
  switch (status) {
    case "verified-locally":
      return "local";
    case "node-reported":
      return "node";
    case "unavailable":
      return "n/a";
  }
}
