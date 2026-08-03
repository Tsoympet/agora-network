/** Shared Agora brand tokens for TS clients (desktop / mobile / explorer). */
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
    TLT: { name: "Talanton", meaning: "Scales of value" },
    DRC: { name: "Drachma", meaning: "Corinthian helm" },
    OVL: { name: "Ovolos", meaning: "Winged helm / spears" },
  },
} as const;

export type AgoraBrand = typeof agoraBrand;
