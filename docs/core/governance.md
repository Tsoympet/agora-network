# Civic governance (`agora-governance`)

> **Status: administrative prototype.** The engine + JSON-RPC surface today are
> **local node Meta-CF state**, not secured blockchain governance. RPC callers
> supply voter address, claimed balances/supply, and slots; votes are not signed,
> deposits are not locked UTXOs, tallies can run before deadlines, and most
> “executed” proposals have no protocol side effects. Different nodes can diverge.
> Before calling this on-chain governance it needs signed governance txs, consensus
> balance snapshots, locked deposits, height deadlines, deterministic execution,
> governance state roots, and block-replicated state. See the deferred list in
> [`PATH_TO_COMPLETE_CHAIN.md`](../governance/PATH_TO_COMPLETE_CHAIN.md).

Agora’s civic model — **Constitution**, **elected ranks**, **voting chambers**,
and **proposal lifecycle** — plus launch-security vote math (quadratic + whale
cap). The crate is the intended protocol shape; node wiring is still soft-auth.

Human-readable charter: [`docs/governance/CONSTITUTION.md`](../governance/CONSTITUTION.md).

## Compared to other chains

| Piece | Agora | Analog |
| --- | --- | --- |
| Higher law | Constitution v1 (`constitution-v1` + content hash) | Tezos amendment / EOS constitution |
| Lifecycle | Deposit → Voting → Tally → Timelock → Execute | Cosmos Hub `x/gov` |
| Tracks / where votes happen | **Ecclesia** / **Boule** / **Archon Collegium** | Polkadot OpenGov tracks + council |
| Vote weight (Ecclesia) | `⌊√capped⌋` after 5% supply whale cap | Quadratic voting (launch security) |
| Officers | Archons, Bouleutai, Tamiai (Greek ranks) | Council / fellowship seats |

Bitcoin/Ethereum-style **off-chain** rough consensus still applies to client
software; this crate defines **binding civic acts** once the node wires the
engine to state/RPC.

## Elected ranks

| Rank | Greek | Seats (v1) |
| --- | --- | --- |
| Archon Eponymous | Ἄρχων Ἐπώνυμος | 1 |
| Archon Basileus | Ἄρχων Βασιλεύς | 1 |
| Archon Polemarch | Ἄρχων Πολέμαρχος | 1 |
| Bouleutes | Βουλευτής | 21 |
| Tamias | Ταμίας | 3 |

Code: `CivicRank`, `OfficeBoard`.

## Where proposals are voted

| Chamber | Who | Weight |
| --- | --- | --- |
| **Ecclesia** | All TLT holders | Quadratic + whale cap |
| **Boule** | Seated Bouleutai (+ Archons) | 1 seat = 1 vote |
| **Archon Collegium** | Three Archons | 1 Archon = 1 vote |

`primary_chamber(kind)` maps each `ProposalKind` to a chamber (Constitution Art. IV).

## Proposal kinds

`TextSignal`, `ParameterChange` (minor→Boule / major→Ecclesia), `TreasurySpend`
(needs Tamias sponsor), `SoftwareUpgrade`, `RankElection`, `RankImpeachment`,
`ConstitutionAmendment` (needs Basileus or 2-of-3 Archon assent),
`EmergencyAction` (Archon Collegium).

## Engine API

```rust
use agora_governance::{GovernanceState, ProposalKind, CivicRank, VoteChoice};

let mut gov = GovernanceState::genesis(/* eligible ecclesia power */ 10_000);
let id = gov.submit_proposal(author, "title", "summary", kind, /*slot*/ 0)?;
gov.add_deposit(id, gov.params.min_deposit)?;
gov.open_voting(id, now)?;
gov.cast_vote(id, voter, VoteChoice::Yes, raw_balance, total_supply)?;
gov.tally(id)?;
gov.enter_timelock(id, now)?;
gov.execute(id, now + gov.params.timelock_slots)?;
```

## Quadratic voting (launch security)

```
capped = min(raw_balance, total_supply * 5 / 100)
effective = isqrt(capped)
```

| Item | Role |
| --- | --- |
| `quadratic_votes` | Pure √ mapping |
| `apply_whale_cap` | 5% supply clamp |
| `tally_quadratic_votes` | Electorate → `EffectiveVote` rows |

## Status

| Layer | Status |
| --- | --- |
| Constitution text + hash | yes (document) |
| Ranks / chambers / proposal engine | yes (in-process) |
| Community board (forum + acks) | yes (local) |
| Node Meta CF persistence (`meta/governance`) | yes — **local**, not consensus-derived |
| Canonical authorization policy commitment | yes — genesis/state-root scaffold |
| TLT/OVL/DRC protocol treasury identities + balances | yes — genesis balances plus signed `TreasuryDisbursement` spend |
| Hub/Passport/Grant/Mission schemas + bounded root | yes — canonical read-only scaffold |
| JSON-RPC civic methods | yes — **trusted caller** prototype |
| Explorer ballot panel + desktop vote UI | yes (drives local RPC) |
| Signed votes / locked deposits / block replication | **must stay local-admin** until new envelopes exist |
| L2/L3 operator sets as Ecclesia ranks | out of scope |

## Canonical Phase 5a scaffold

`agora-state-machine` now initializes a separate
`governance/consensus/policy` record plus three fixed-denomination treasuries:

| Treasury | Asset |
| --- | --- |
| `tlt_security` | TLT |
| `ovl_builder` | OVL |
| `drc_community` | DRC |

Their authorization policy and balances commit into the Trident state root.
`agora_getProtocolTreasuries` exposes policy, balances, controller, and nonce.
Controller-signed `TreasuryDisbursement` (protocol v31) debits an existing
treasury only; it never mints. TLT creates a UTXO at the beneficiary; OVL/DRC
credit the account module.

The committed policy catalog includes the Trident OVL Technical, DRC Community,
Ecclesia, miner-signaling, and limited Security Council paths from
`AGORA_CONSTITUTION.md`. It records multi-chamber upgrade requirements,
treasury-specific chambers, grant milestone/COI requirements, extended
timelocks, and mandatory emergency expiry/post-action ratification.

No unsigned civic RPC can mutate these records. Existing `meta/governance`
proposal/forum endpoints remain `administrative_local` and excluded from the
canonical governance root.

### Why civic votes cannot be a v32+ consensus lane yet

The live civic types (`CivicSnapshot`, `Ballot`, `Proposal`, forum topics)
cannot be honestly promoted to BlockDAG gossip:

1. **No secp256k1 envelope.** There is no `chain_id` + genesis-bound signing
   domain, public key, signature, or nonce. Wrapping the JSON blob would
   still leave vote weight as a trusted RPC argument.
2. **Vote weight is caller-supplied.** `agora_castGovVote` takes
   `raw_balance` and `total_supply` from the HTTP caller. Consensus would
   need a TLT-UTXO-derived weight proof at a committed blue-score, not an
   operator-chosen integer.
3. **Not journaled or rooted.** Persistence is pretty-printed JSON at
   `meta/governance`. Apply/revert, compact lanes, and
   `governance_treasury_root` do not see it. A reorg cannot undo a civic
   vote because the vote never entered the state machine.
4. **Lifecycle is an in-process engine.** Deposit, open, tally, timelock,
   and execute are node-local method calls. A honest consensus civic
   protocol would need new signed types for each of those acts, plus
   chamber quorums that match the constitution catalog already committed
   at genesis.

Hub/Grant/Mission/Passport/Treasury/Vesting succeeded because those
envelopes already had (or could be given) a single signer and a
deterministic state write. Civic voting does not. Promoting it now would
be a new protocol, not a completion of existing types. Signed treasury
*spend* exists; signed treasury *funding* and signed Ecclesia ballots do
not.
