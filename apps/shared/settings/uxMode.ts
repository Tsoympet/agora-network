/**
 * Beginner copy avoids consensus vocabulary.
 * Advanced surfaces name the tools that exist, and mark contract calls PLANNED.
 */

import { phishingWarnings } from "../security/phishing";
import { DATA_STORES } from "../core/stores";

export type UxMode = "beginner" | "advanced";

export const UX_MODE_STORAGE_KEY = "agora.community.uxMode.v1";

export const ASSET_COPY = {
  drc: "DRC PAYMENTS",
  ovl: "OVL APPLICATIONS",
  tlt: "TLT SECURITY/VALUE",
  community: "COMMUNITY PARTICIPATION",
} as const;

export const BEGINNER_SURFACES = [
  { id: "wallet.create", label: "Create wallet" },
  { id: "wallet.import", label: "Import wallet" },
  { id: "passport", label: "Passport" },
  { id: "drc.receive", label: "Receive DRC" },
  { id: "drc.pay", label: "Pay" },
  { id: "community", label: "Community" },
  { id: "mission.first", label: "First mission" },
  { id: "academy", label: "Academy" },
  { id: "merchants", label: "Merchants" },
  { id: "projects", label: "Projects" },
  { id: "guild.join", label: "Join Guild" },
] as const;

export const ADVANCED_SURFACES = [
  { id: "rpc", label: "RPC", status: "present" },
  { id: "raw-tx", label: "Raw transactions", status: "present" },
  { id: "contract-call", label: "Contract calls", status: "PLANNED" },
  { id: "utxo", label: "UTXO details", status: "present" },
  { id: "proofs", label: "Proofs", status: "present" },
  { id: "network", label: "Network", status: "present" },
  { id: "devtools", label: "Developer tools", status: "present" },
] as const;

const JARGON = /\b(GHOSTDAG|quorum|blue score|UTXO|Merkle|RandomX|header spine|RPC|nonce)\b/i;

export function parseUxMode(raw: string | null | undefined): UxMode {
  return raw === "advanced" ? "advanced" : "beginner";
}

export function beginnerNavText(): string {
  return [...Object.values(ASSET_COPY), ...BEGINNER_SURFACES.map((item) => item.label)].join("\n");
}

export function beginnerHasConsensusJargon(text: string): boolean {
  return JARGON.test(text);
}

export type PanelSurface = { id: string; label: string; status?: "present" | "PLANNED" };

export function architecturePanelModel(mode: UxMode, nodeUrl: string): {
  mode: UxMode;
  copy: typeof ASSET_COPY;
  surfaces: readonly PanelSurface[];
  warnings: string[];
  stores: readonly string[];
  hardwareWallet: "PLANNED";
  seedOnScreen: false;
} {
  const surfaces: PanelSurface[] =
    mode === "beginner"
      ? BEGINNER_SURFACES.map((item) =>
          item.id === "projects" ? { ...item, status: "PLANNED" } : { ...item },
        )
      : ADVANCED_SURFACES.map((item) => ({ ...item }));
  return {
    mode,
    copy: ASSET_COPY,
    surfaces,
    warnings: phishingWarnings(nodeUrl),
    stores: DATA_STORES,
    hardwareWallet: "PLANNED",
    seedOnScreen: false,
  };
}
