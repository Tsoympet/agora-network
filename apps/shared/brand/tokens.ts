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
  /** L3 native PoW — district / bridge money (sha256_leading_zero). */
  DRC: {
    name: "Drachma",
    layer: "L3",
    maxSupplyWhole: 6_000_000_000,
    decimals: 8,
    role: "XRP-class payments rail / district path payments / bridge liquidity",
    native: true,
    powAlgorithm: "sha256_leading_zero",
  },
  /** L2 native PoW — Ethereum-class EVM gas money (sha256_leading_zero). */
  OVL: {
    name: "Ovolos",
    layer: "L2",
    maxSupplyWhole: 21_000_000_000,
    decimals: 8,
    role: "Ethereum-class L2 gas + EVM execution money",
    native: true,
    powAlgorithm: "sha256_leading_zero",
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
      name: "Talanton",
      meaning: "Scales of value",
      ...agoraTokenSupplies.TLT,
    },
    DRC: {
      name: "Drachma",
      meaning: "Corinthian helm",
      ...agoraTokenSupplies.DRC,
    },
    OVL: {
      name: "Ovolos",
      meaning: "Winged helm / spears",
      ...agoraTokenSupplies.OVL,
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
