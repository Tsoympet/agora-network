/**
 * Clipboard policy. Restore words may be copied only after an explicit reveal,
 * and the caller should clear them. Addresses are not seed material.
 */

export type ClipboardKind = "address" | "mnemonic" | "pairing-watch" | "pairing-restore";

export type ClipboardDecision = {
  allow: boolean;
  clearAfterMs: number | null;
  warning: string;
};

export function clipboardDecision(kind: ClipboardKind, explicitReveal: boolean): ClipboardDecision {
  if (kind === "mnemonic" || kind === "pairing-restore") {
    if (!explicitReveal) {
      return {
        allow: false,
        clearAfterMs: null,
        warning: "Seed clipboard copy requires an explicit reveal.",
      };
    }
    return {
      allow: true,
      clearAfterMs: 30_000,
      warning: "Clear the clipboard after the other device has the words. A pasteboard is not private.",
    };
  }
  return { allow: true, clearAfterMs: null, warning: "" };
}
