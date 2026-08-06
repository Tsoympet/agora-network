# Canonical genesis

## Trident L1 (target)

Agora Trident freezes **one** genesis document for the hybrid L1 with three native assets.

| Network | Artifact | Status |
| --- | --- | --- |
| Trident testnet | [`trident.testnet.genesis.draft.json`](trident.testnet.genesis.draft.json) | **Draft** (not frozen; Phase 1+) |
| Trident mainnet | TBD | Not bootable until human freeze |

See [`../architecture/TRIDENT_L1.md`](../architecture/TRIDENT_L1.md) and [`../migration/OVL_DRC_TO_L1.md`](../migration/OVL_DRC_TO_L1.md).

Working supply caps (8 decimals): TLT 100M · OVL 21B · DRC 6B whole units. Only **TLT** is mineable.

## Historical artifacts (pre-Trident)

These remain for reproducibility of the layered lab stack. They are **not** the Trident monetary root.

| Layer (historical) | Mark | Artifact (testnet) | Artifact (mainnet draft) |
| --- | --- | --- | --- |
| L1 BlockDAG | TLT | [`testnet.genesis.json`](testnet.genesis.json) **frozen v2** | [`mainnet.genesis.draft.json`](mainnet.genesis.draft.json) |
| L2 Ovolos lab | OVL | [`ovolos.testnet.genesis.json`](ovolos.testnet.genesis.json) | [`ovolos.mainnet.genesis.draft.json`](ovolos.mainnet.genesis.draft.json) |
| L3 Drachma lab | DRC | [`drachma.testnet.genesis.json`](drachma.testnet.genesis.json) | [`drachma.mainnet.genesis.draft.json`](drachma.mainnet.genesis.draft.json) |

Frozen L1 testnet v2 genesis hash:

```text
afe59232cd20a16bd56948044149d2b8013e63f3694c113074fef75ab0cb9b98
```

Trident requires genesis **v3**, a new `chain_id`, and a new network fingerprint — peers do not silently upgrade from v2.

## Wallet identity (L1 addresses)

| Network | Bech32m HRP | BIP-44 coin type |
| --- | --- | --- |
| mainnet | `agora` | `8888` (provisional SLIP-0044) |
| testnet | `agoratest` | `8888` |
| dev | `agoradev` | `8888` |

Trident wallets use one seed with **separated derivation roles** per asset/validator function (Phase 1+).

## CLI (current L1 v2)

```bash
cargo run -p agora-node -- genesis verify --network testnet
```
