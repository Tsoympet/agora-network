# Validator Runbook (Trident)

**Maturity:** Scaffold. Applies when OVL/DRC staking + finality are implemented (Phase 3+).

## Roles

Operators may run **OVL validators**, **DRC validators**, or both (separate keys and stake).

## Checklist

1. Verify network fingerprint / chain ID / genesis hash before unlocking keys.  
2. Store consensus keys offline or in HSM/hardware where possible; withdrawal address separate.  
3. Meet self-bond and metadata requirements; monitor jail status.  
4. Sign only domain-separated checkpoints for the active epoch.  
5. Never reuse validator keys across networks.  
6. Publish uptime and stewardship metrics to the Node Guild directory when available.  
7. Incident path: see [`INCIDENT_RESPONSE.md`](INCIDENT_RESPONSE.md).

## Non-goals for early testnets

Do not enable public mint RPCs. Do not claim mainnet readiness.
