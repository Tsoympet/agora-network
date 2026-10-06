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
| TLT | Talanton | Balanced scales | 100,000,000 | L1 native PoW (RandomX) |
| DRC | Drachma | Crested Corinthian helm | 6,000,000,000 | L1 native contract-free account/payment asset (never mined) |
| OVL | Ovolos | Winged helm, crossed spears | 21,000,000,000 | L1 native execution/gas asset (never mined) |

The displayed quantities mirror existing brand/genesis metadata; this brand
document does not set Trident monetary policy. Only TLT is mined. OVL is the
sole smart-contract/VM domain; DRC escrow, Checks, payment channels, trust
lines, issued assets, freeze/clawback, multisign, and Tickets are typed native
operations rather than contracts.

## Rules

1. Brand name must read as the hero-level signal on promotional surfaces.
2. Do not invent alternate purple / cream palettes.
3. Prefer full-bleed atmospheric backgrounds over flat gray panels.
4. Prefer photoreal coin PNGs over simplified SVG placeholders for token UI.
