# DRC Tickets (rippled 2.5.0 subset)

**Maturity:** Experimental · Single-node prototype  
**Not XRPL wire/API parity** — no `TicketBatch`, reserve math, or ledger object wire shapes.

## Pinned rippled intent (2.5.0)

Rippled **Tickets** let an account enqueue future sequence numbers so signed transactions can execute **out of order** while each ticket is **single-use**. Creation consumes sequence space; spending a ticket does not advance the account’s current sequence.

## Agora Trident subset (phase 2 — in progress on `cursor/drc-tickets-cdcf`)

| Topic | Agora behavior |
|--------|----------------|
| Scope | DRC account lanes only; OVL/TLT unchanged |
| Create | Dedicated `DrcTicketCreateTx` lane; **exactly one** ticket per operation |
| Create auth | Master / regular / multisign (same central verifier); **nonce-only** (never consumes a ticket) |
| Sequence rule | Create at ordinary nonce `N` mints ticket sequence `N+1`, advances account nonce to `N+2` (checked overflow) |
| Cap | `DRC_MAX_OUTSTANDING_TICKETS_PER_ACCOUNT = 32` (no account reserve in this slice) |
| Spend | New operation versions bind `DrcAccountSequenceSelector`: `Nonce(n)` or `Ticket(seq)` in signing + Borsh |
| Ticket spend | Removes ticket; **does not** bump ordinary nonce |
| Nonce spend | Unchanged sequential semantics |
| Expiry/cancel | **None** in this slice; cap bounds live state |
| State | Sorted per-owner ticket sets committed in `agora-drc-ticket-root-v1` inside composed state root |
| Body | `agora-block-body-v12` when `drc_ticket_creates` non-empty |

### Lane order (same block)

1. Regular keys  
2. Signer lists  
3. **Ticket creates**  
4. Account transfers  
5. Stake  
6. OVL execution  
7. Policy  
8. Deposit preauth  
9. Payments  
10. Multisign attachments  

Same-block **create then spend** is supported when create lanes run before spend lanes in that block (overlay visible to later lanes).

### Deviations from rippled

- No batch ticket creation, cancellation, or expiration  
- Hard cap 32 instead of owner reserve  
- secp256k1-only; no `lsf` / `tec` classes  
- Explicit versioned envelopes per operation family (no unsigned sidecar)

### Version artifacts (target)

- Trident protocol **v15**, tx signing **v9**, state transition **`agora-trident-state-v16`**, body **v12**  
- Account transfer **v3** (DRC ticket selector) — **landed**  
- Remaining DRC lanes: stake, regular key, signer list, policy, preauth, payment **v+1 with selector** — **tracked** on branch

### RPC (target)

- `agora_submitDrcTicketCreate`, `agora_getDrcAccountTickets` (point lookup, no enumeration)  
- Malformed params: `-32602`
