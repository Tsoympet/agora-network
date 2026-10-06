/**
 * Client folders. `community` is the existing product module from the
 * ecosystem slice. The other folders are the architecture split and re-export
 * that module instead of moving it.
 */

export const CLIENT_MODULES = [
  "core",
  "wallet",
  "lightclient",
  "drc",
  "ovl",
  "tlt",
  "passport",
  "community",
  "missions",
  "academy",
  "grants",
  "bounties",
  "guilds",
  "merchants",
  "events",
  "assembly",
  "treasury",
  "forum",
  "notifications",
  "explorer",
  "security",
  "settings",
] as const;

export type ClientModule = (typeof CLIENT_MODULES)[number];
