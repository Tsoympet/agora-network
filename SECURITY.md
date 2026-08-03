# Security Policy

## Supported networks

| Network | Status |
| --- | --- |
| `dev` | Local development only |
| `testnet` | Frozen genesis in-repo; treat as experimental |
| `mainnet` | **Not frozen** — node refuses to boot |

Do **not** bridge real value to Agora until mainnet genesis is frozen and an
external review is published (see [`docs/governance/PATH_TO_COMPLETE_CHAIN.md`](docs/governance/PATH_TO_COMPLETE_CHAIN.md)).

## Reporting a vulnerability

Please open a **private** security advisory on GitHub (Security → Advisories)
or contact the repository maintainers. Do not file public issues for
exploitable consensus, wallet, or RPC flaws.

Include:

- Affected component (`agora-node`, P2P, RPC, wallet, etc.)
- Network (`dev` / `testnet`)
- Reproduction steps and impact assessment

## Scope notes

- `agora_fundAddress` is a **dev/testnet mint** and is hard-disabled on mainnet.
- Public RPC binds require `AGORA_RPC_ALLOW_PUBLIC_BIND=1`; use `AGORA_RPC_TOKEN`
  and TLS termination in front of any exposed endpoint.
- Vendored `agora-kheavyhash` is ISC-licensed Kaspa code; report algorithm bugs
  upstream when appropriate.
