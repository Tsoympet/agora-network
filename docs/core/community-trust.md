# Community trust module

**Maturity:** Scaffold.

`core/crates/governance/src/community_trust.rs` holds the device-side rules for
local community attributes, proposal transparency, reward funding limits,
public analytics aggregates, and portable identity export.

The algorithm is intentionally small:

1. Community search filters seed or caller-supplied records by typed text.
   Country is a string. Coordinate keys are rejected.
2. Proposal badges are a function of `on_chain` or `advisory`. Advisory
   records cannot take a binding final result.
3. Reward evaluation never sets a supply delta. TLT sourced from PoW emission
   is marked `ui_blocked`. Treasury-funded amounts must already be covered.
4. Analytics parsing accepts only the fifteen public counters.
5. Identity export builds JSON on the caller side and refuses key material.
   The bundle embeds the ten light-client trust assumptions.

No step reads GPS, admits a block, or changes TLT monetary policy.

User-facing detail: [`docs/community/COMMUNITY_TRUST.md`](../community/COMMUNITY_TRUST.md).
Trust assumptions: [`LIGHT_CLIENT_SECURITY_MODEL.md`](LIGHT_CLIENT_SECURITY_MODEL.md).
