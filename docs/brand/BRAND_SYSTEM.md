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
| `apps/shared/brand/assets/agora-network.png` | Primary brand mark (column `A` + wordmark) |
| `apps/shared/brand/assets/agora-app-icon.png` | App / favicon square crop |
| `apps/shared/brand/assets/nexus-icon.png` | Alias of brand mark for legacy paths |
| `apps/shared/brand/assets/talanton.png` | TLT — gold scales coin |
| `apps/shared/brand/assets/drachma.png` | DRC — silver Corinthian helm coin |
| `apps/shared/brand/assets/ovolos.png` | OVL — bronze winged-helm coin |
| `apps/shared/brand/assets/masters/` | Full-resolution source masters |

## Client wiring

- **Explorer:** `Agora_Brand_System.css`; favicon `agora-app-icon.png`; marks under `public/brand/*.png`
- **Desktop (Tauri):** `src-tauri/icons/*` from app icon; frontend uses `agora-network.png`
- **Mobile (Expo):** `app.json` `icon` / `splash` / adaptive icon → `assets/icon.png`

## Marks

| Ticker | Name | Motif | Max supply | Layer |
| --- | --- | --- | --- | --- |
| TLT | Talanton | Balanced scales | 100,000,000 | L1 native |
| DRC | Drachma | Crested Corinthian helm | 6,000,000,000 | L2+ registry |
| OVL | Ovolos | Winged helm, crossed spears | 21,000,000,000 | L2 registry |

Supplies are frozen in `docs/genesis/*.genesis.json` (`tokens[]`). Only TLT is the L1 `Amount` today.

## Rules

1. Brand name must read as the hero-level signal on promotional surfaces.
2. Do not invent alternate purple / cream palettes.
3. Prefer full-bleed atmospheric backgrounds over flat gray panels.
4. Prefer photoreal coin PNGs over simplified SVG placeholders for token UI.
