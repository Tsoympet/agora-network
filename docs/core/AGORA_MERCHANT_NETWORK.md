# Agora Merchant Network

**Maturity:** Scaffold

## Purpose

The merchant network is how people pay and get paid in DRC: directory, invoices, QR, and local hubs. Settlement is native DRC. This file is the readiness note. The earlier scaffold remains [`docs/community/MERCHANT_NETWORK.md`](../community/MERCHANT_NETWORK.md).

## Data sources

| Label | Role on this base |
| --- | --- |
| blockchain | DRC payments, destination tags, escrow, checks, and payment channels exist as L1 types and, on the light client, as signed envelopes or node-reported queries. See `featureMatrix.ts`. |
| community | A merchant profile (display name, region, receiving address) would be community-submitted. No profile store is on this commit. |
| indexer | A map or directory index is not present. Explorer transaction lookup is not a merchant map. |
| private | The merchant spend key stays in the merchant’s vault. A directory entry is a receiving address. |

## Trust boundary

The light client can sign a DRC payment envelope on device when the wallet is unlocked, and it fails closed for watch-only. It does not prove DRC account state against a header. A merchant screen that showed a directory balance as verified would be false. This slice does not add that screen.

## Maturity

Scaffold for the merchant product. DRC ledger features on the base are ahead of the directory and are documented in the DRC core notes, not re-labeled here.

Inspected on `ed72a27` (`origin/cursor/agora-light-client-surfaces-cdcf`, PR #153). Uncommitted work in other worktrees is outside this base.

## Implemented

- The capability list in `docs/community/MERCHANT_NETWORK.md`.
- DRC typed signing and object queries in the light-client feature matrix, with node-reported or unavailable labels.

## Planned

- Receiving profiles, invoices, QR payment requests, and tests that a profile cannot carry a spend key.
- Fee sponsorship remains a wallet concern. Consensus still has no price oracle.

## Related

- Map: [`AGORA_COMMUNITY_ARCHITECTURE.md`](AGORA_COMMUNITY_ARCHITECTURE.md).
- Phases: [`AGORA_COMMUNITY_PHASES.md`](AGORA_COMMUNITY_PHASES.md).
- Checklist: [`AGORA_COMMUNITY_DEFINITION_OF_DONE.md`](AGORA_COMMUNITY_DEFINITION_OF_DONE.md).
- Prior note: [`docs/community/MERCHANT_NETWORK.md`](../community/MERCHANT_NETWORK.md).
- DRC payments: [`drc-payments.md`](drc-payments.md).

