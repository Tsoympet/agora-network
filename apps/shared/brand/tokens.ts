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
    nexus: "nexus-icon.svg",
    talanton: "talanton.svg",
    drachma: "drachma.svg",
    ovolos: "ovolos.svg",
  },
  marks: {
    TLT: { name: "Talanton", meaning: "Scales" },
    DRC: { name: "Drachma", meaning: "Helmet" },
    OBL: { name: "Ovolos", meaning: "Shield / Spears" },
  },
} as const;

export type AgoraBrand = typeof agoraBrand;
