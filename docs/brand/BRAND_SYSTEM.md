# Agora Brand System

Obsidian & Gold visual identity for all Agora clients.

## Tokens

| Token | Value | Use |
| --- | --- | --- |
| Agora Obsidian | `#101218` | Primary background |
| Burnished Gold | `#C59835` | Brand / CTA / Nexus |
| Aegean Cyan | `#06BBDF` | Secondary accent |
| Display | Cinzel | Brand wordmark, section titles |
| UI | Inter | Body, controls |

## Files

| Path | Role |
| --- | --- |
| `apps/shared/brand/Agora_Brand_System.css` | CSS variables, type, buttons, motion |
| `apps/shared/brand/tokens.ts` | TS token mirror for React Native / logic |
| `apps/shared/brand/assets/nexus-icon.svg` | Primary app icon (gold `A`) |
| `apps/shared/brand/assets/talanton.svg` | TLT — Scales |
| `apps/shared/brand/assets/drachma.svg` | DRC — Helmet |
| `apps/shared/brand/assets/ovolos.svg` | OBL — Shield / Spears |

## Client wiring

- **Explorer:** imports `Agora_Brand_System.css`; public assets copied/linked from shared brand
- **Desktop (Tauri):** `src-tauri/icons/icon.png` (and SVG source) = Nexus; frontend imports brand CSS
- **Mobile (Expo):** `app.json` `icon` / `splash` point at Nexus asset

## Rules

1. Brand name must read as the hero-level signal on promotional surfaces.
2. Do not invent alternate purple / cream palettes.
3. Prefer full-bleed atmospheric backgrounds over flat gray panels.
