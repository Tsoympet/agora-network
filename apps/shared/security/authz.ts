/**
 * Role checks for light-client actions.
 * Seed export is never granted by a role. The device session's explicit
 * reveal path is the only export, and it still fails when locked.
 */

export type ClientRole = "locked" | "watch-only" | "spend";

export type ClientAction = "read" | "sign" | "export-seed" | "community-read";

export function authorize(
  role: ClientRole,
  action: ClientAction,
): { allowed: boolean; reason?: string } {
  if (action === "export-seed") {
    return { allowed: false, reason: "session cannot export seed" };
  }
  if (action === "community-read") return { allowed: true };
  if (role === "locked") return { allowed: false, reason: "wallet is locked" };
  if (action === "read") return { allowed: true };
  if (action === "sign") {
    if (role !== "spend") {
      return { allowed: false, reason: "watch-only wallets cannot sign" };
    }
    return { allowed: true };
  }
  return { allowed: false, reason: "denied" };
}

const PRIVATE_MARKERS = ["mnemonic", "seed", "xprv", "password", "privatekey", "secretkey"];

/** Community and indexer payloads must not carry key material. */
export function assertPublicPayload(payload: Record<string, unknown>): void {
  for (const key of Object.keys(payload)) {
    const norm = key.toLowerCase().replace(/[_-]/g, "");
    if (PRIVATE_MARKERS.some((marker) => norm.includes(marker))) {
      throw new Error(`private field ${key} must not leave the device`);
    }
  }
}
