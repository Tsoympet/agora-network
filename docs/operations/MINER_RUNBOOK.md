# Miner Runbook (Trident)

**Maturity:** Scaffold / Experimental (TLT RandomX path exists on `main`).

## Role

TLT RandomX miners propose and order blocks. Mining does **not** mint OVL or DRC.

## Checklist

1. Confirm `AGORA_NETWORK`, genesis hash, and network fingerprint.  
2. Public networks: RandomX only — no silent algorithm fallback.  
3. Use `agora_getBlockTemplate` / submit paths bound to the fingerprint.  
4. Monitor template work ranking and fee inclusion (accepted-fee aware once Phase 2 lands).  
5. Miner signaling for upgrades does not control community treasuries.

## Related

- Existing node/miner docs under `docs/core/` and `docs/ops/PUBLIC_TESTNET.md`  
- Sidecar: `agora-miner-sidecar`  
