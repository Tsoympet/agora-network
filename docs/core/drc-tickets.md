# DRC Tickets (rippled 2.5.0 subset)

**Maturity:** Experimental · Single-node prototype  
**Public submission:** **Stage B** — mempool reservation, gossip, mining templates, `agora_submitDrcTicketCreate`, `agora_getDrcTicket`.  
**Not XRPL wire/API parity** — no `TicketBatch`, reserve math, or ledger object wire shapes.

## Pinned rippled intent (2.5.0)

Rippled **Tickets** let an account enqueue future sequence numbers so signed transactions can execute **out of order** while each ticket is **single-use**. Creation consumes sequence space; spending a ticket does not advance the account’s current sequence.

## Agora Trident subset

| Topic | Agora behavior |
|--------|----------------|
| Scope | DRC account lanes only; OVL/TLT unchanged |
| Create | Dedicated `DrcTicketCreateTx` lane; **exactly one** ticket per operation |
| Create auth | Master / regular / multisign (same central verifier); **nonce-only** (never consumes a ticket) |
| Sequence rule | Create at ordinary nonce `N` mints ticket sequence `N+1`, advances account nonce to `N+2` |
| Cap | `DRC_MAX_OUTSTANDING_TICKETS_PER_ACCOUNT = 32` |
| Spend | Ticket-capable operation versions bind `DrcAccountSequenceSelector`: `Nonce(n)` or `Ticket(seq)` in signing + Borsh |
| Ticket spend | Removes ticket; **does not** bump ordinary nonce |
| Nonce spend | Unchanged sequential semantics |
| Atomic helper | `begin_drc_account_sequence` validates; `finish_drc_account_sequence` commits only after all later checks pass |
| State | Sorted per-owner ticket sets in `agora-drc-ticket-root-v1` inside composed state root |
| Body | `agora-block-body-v12` when `drc_ticket_creates` non-empty |

### Ticket-capable operation versions

| Family | Version |
|--------|---------|
| Account transfer | **v3** |
| Stake (DRC) | **v2** |
| Regular key | **v2** |
| Signer list | **v2** |
| Account policy | **v4** (all actions) |
| Deposit preauth | **v2** |
| Payment | **v5** |

Legacy versions remain byte-for-byte; `Ticket` selectors on legacy versions are rejected.

### Canonical mined lane order (same block)

1. **Ticket create** (uses **pre-block** regular-key / signer-list / policy for auth)  
2. Account transfer  
3. OVL execution  
4. Stake  
5. Regular key  
6. Signer list  
7. Account policy  
8. Deposit preauth  
9. Payment  
10. Data availability  

**Tradeoff:** Ticket creation intentionally uses pre-block key/list/policy state so it cannot be authorized by signer-list or policy mutations in the same block. Tickets minted in step 1 are visible to all later lanes via the copy-on-write overlay (including spend in transfer/stake/payment/etc.).

Multisign attachments (consensus lane, after body lanes in block encoding) include `DrcTicketCreate` operation kind **8** with the same signing-commitment rules as other DRC ops.

### Stage B public admission (mempool / RPC — not consensus authority)

| Policy | Behavior |
|--------|----------|
| Ticket create | Reserves owner account nonce **and** prospective ticket sequence `N+1` until eviction or inclusion |
| Nonce spend | One pending operation per DRC owner (unchanged) |
| Ticket spend | One pending consumer per `(owner, ticket_sequence)`; ticket must be **live on canonical state** |
| Same-block create→use via public paths | **Rejected** — spend before create confirms returns mempool error; miners may still pair create+use in one **consensus** block |
| RPC submit | `agora_submitDrcTicketCreate` with param `ticket_create` (Borsh/JSON envelope) |
| RPC lookup | `agora_getDrcTicket(owner, ticket_sequence)` → `status`: `live` or `unknown` (consumed vs never-created not distinguished); malformed params → JSON-RPC `-32602` |
| Gossip | `NetworkMessage::DrcTicketCreate` (Trident protocol **v15** mesh) |

**Exclusions (unchanged):** no batch create, cancel, expiry, or ticket enumeration.

### Deviations from rippled

- No batch ticket creation, cancellation, or expiration  
- Hard cap 32 instead of owner reserve  
- secp256k1-only  
- Explicit versioned envelopes per operation family

### Version artifacts

- Trident protocol **v15**, tx signing **v9**, state transition **`agora-trident-state-v16`**, body **v12**  
- `UtxoJournal` migration **v9** adds `drc_ticket_meta_before` for reorg rollback
