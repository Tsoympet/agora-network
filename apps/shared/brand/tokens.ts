/** Shared Agora brand tokens for TS clients (desktop / mobile / explorer). */

/** Max supplies in whole units (8 decimals on-chain). */
export const agoraTokenSupplies = {
  /** L1 native PoW — BlockDAG UTXO settlement asset (RandomX). */
  TLT: {
    name: "Talanton",
    layer: "L1",
    maxSupplyWhole: 100_000_000,
    decimals: 8,
    role: "native store of value / BlockDAG settlement",
    native: true,
    powAlgorithm: "randomx",
  },
  /** L1 native PoS collateral and contract-free payment/state-machine asset. */
  DRC: {
    name: "Drachma",
    layer: "L1",
    maxSupplyWhole: 6_000_000_000,
    decimals: 8,
    role: "contract-free payments / typed settlement / community validators",
    native: true,
    powAlgorithm: "none",
  },
  /** L1 native PoS collateral and sole programmable-execution gas asset. */
  OVL: {
    name: "Ovolos",
    layer: "L1",
    maxSupplyWhole: 21_000_000_000,
    decimals: 8,
    role: "smart-contract execution gas / builders / technical validators",
    native: true,
    powAlgorithm: "none",
  },
} as const;

export const agoraBrand = {
  colors: {
    obsidian: "#101218",
    obsidianElevated: "#171B24",
    gold: "#C59835",
    goldSoft: "#D4AF5A",
    cyan: "#06BBDF",
    ink: "#E8E6E1",
    inkMuted: "#9AA0AB",
  },
  fonts: {
    display: "Cinzel, Times New Roman, serif",
    ui: "Inter, Segoe UI, sans-serif",
  },
  assets: {
    nexus: "nexus-icon.png",
    agoraNetwork: "agora-network.png",
    appIcon: "agora-app-icon.png",
    talanton: "talanton.png",
    drachma: "drachma.png",
    ovolos: "ovolos.png",
  },
  marks: {
    TLT: {
      ...agoraTokenSupplies.TLT,
      meaning: "Scales of value",
    },
    DRC: {
      ...agoraTokenSupplies.DRC,
      meaning: "Corinthian helm",
    },
    OVL: {
      ...agoraTokenSupplies.OVL,
      meaning: "Winged helm / spears",
    },
  },
  wallet: {
    coinType: 8888,
    coinTypeStatus: "provisional-slip44-pending",
    hrp: {
      mainnet: "agora",
      testnet: "agoratest",
      dev: "agoradev",
    },
  },
} as const;

export type AgoraBrand = typeof agoraBrand;
