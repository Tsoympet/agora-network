# Constitution of the Agora Network

**Version:** 1  
**In-protocol id:** `constitution-v1`  
**Binding asset for civic vote weight:** TLT (Talanton) on L1  

This charter defines how Agora governs itself. It sits above ordinary chain
parameters: amending it requires a **ConstitutionAmendment** proposal that
passes in the **Ecclesia** under the heightened thresholds in Article VIII.

The design borrows structure from classical Athens (Ecclesia / Boule / Archons)
and from modern on-chain systems: Cosmos Hub governance (deposit → vote →
timelock → execute), Polkadot OpenGov (separate tracks / chambers by risk),
and Tezos-style explicit amendment of higher law. Bitcoin/Ethereum-style
off-chain rough consensus remains available for client software; **this
constitution governs on-chain civic acts** once the governance runtime is
enacted.

---

## Article I — Sovereignty of the Ecclesia

1. The **Ecclesia** (Ἐκκλησία) is the assembly of all TLT holders.
2. Final authority over ordinary proposals, elections, treasury spends, and
   constitution amendments rests with the Ecclesia unless this charter
   assigns a prior chamber vote.
3. Vote weight in the Ecclesia is **quadratic** after a **5% whale cap** on
   countable balance vs circulating TLT supply
   (`EffectiveVotes = ⌊√capped_balance⌋`), as implemented in `agora-governance`.

## Article II — The Boule

1. The **Boule** (Βουλή) is a standing council of elected **Bouleutai**.
2. Default seating: **21 Bouleutai**, elected by the Ecclesia for a fixed term.
3. The Boule may:
   - originate and refine proposals;
   - hold **Boule chamber** votes for proposal kinds assigned to it;
   - recommend pass/reject before an Ecclesia referendum when required.
4. Boule chamber voting is **one seat, one vote** (not quadratic).

## Article III — The Archons (elected ranks)

The Ecclesia elects the following officers. Titles are civic ranks, not
aristocratic titles of nobility.

| Rank | Greek | Seats | Portfolio |
| --- | --- | --- | --- |
| **Archon Eponymous** | Ἄρχων Ἐπώνυμος | 1 | Chairs the Boule; tie-break; ceremonial year-name |
| **Archon Basileus** | Ἄρχων Βασιλεύς | 1 | Guardian of this Constitution; sacred / higher-law track |
| **Archon Polemarch** | Ἄρχων Πολέμαρχος | 1 | Security, emergency pause, incident response track |
| **Bouleutes** | Βουλευτής | 21 | Seated members of the Boule |
| **Tamias** | Ταμίας | 3 | Treasury stewards; sponsor / audit spends |

1. An address may hold at most **one** Archon seat at a time, but an Archon
   may also sit as a Bouleutes if separately elected (optional; default
   params forbid dual Archon seats only).
2. Terms, vacancy, and impeachment follow Articles VI–VII.
3. Rank names in protocol code: `ArchonEponymous`, `ArchonBasileus`,
   `ArchonPolemarch`, `Bouleutes`, `Tamias`
   (`agora_governance::ranks::CivicRank`).

## Article IV — Voting chambers (where proposals are voted)

Every proposal is assigned exactly one **primary chamber** in which the
binding vote is cast. Secondary assent may be required.

| Chamber | Who votes | Weight | Used for |
| --- | --- | --- | --- |
| **Ecclesia** | All TLT holders | Quadratic + whale cap | Referenda, elections, treasury, amendments, upgrades |
| **Boule** | Seated Bouleutai (+ Archons if params say so) | 1 seat = 1 vote | Fast-track parameter tweaks, drafting gates |
| **ArchonCollegium** | The three Archons | 1 Archon = 1 vote | Emergency actions; Basileus assent on amendments |

### Proposal → chamber map (v1)

| Proposal kind | Primary chamber | Extra assent |
| --- | --- | --- |
| `TextSignal` | Ecclesia | — |
| `ParameterChange` | Boule *(minor)* or Ecclesia *(major)* | — |
| `TreasurySpend` | Ecclesia | ≥1 Tamias sponsorship |
| `SoftwareUpgrade` | Ecclesia | — |
| `RankElection` | Ecclesia | — |
| `RankImpeachment` | Ecclesia | — |
| `ConstitutionAmendment` | Ecclesia | Archon Basileus non-objection *or* 2-of-3 ArchonCollegium |
| `EmergencyAction` | ArchonCollegium | Ecclesia ratification within emergency window |

Votes are recorded against the proposal id in governance state (in-protocol
store once wired to the node). There is no separate off-chain “forum vote”:
discussion may happen anywhere, but **only chamber votes counted by the
governance engine are binding**.

## Article V — Proposal lifecycle

Aligned with Cosmos Hub `x/gov`:

1. **Draft** — author submits title, body, kind, payload.
2. **Deposit** — minimum TLT deposit (burnable / refundable per params).
3. **Voting** — chamber opens for `voting_period` blocks/slots.
4. **Tally** — quorum + threshold (and veto threshold if enabled).
5. **Passed** → **Timelock** → **Executed** (or **Expired** / **Rejected** /
   **FailedQuorum** / **Vetoed**).

Default v1 thresholds (Ecclesia): quorum **40%** of tallied eligible power
participating; pass threshold **50%** Yes among non-Abstain; veto **33.4%**
NoWithVeto when that option is used. Boule/Archon chambers use simple
majority of seated voters unless params override.

## Article VI — Elections

1. RankElection proposals name the rank, seat index (if multi-seat), and
   candidate address.
2. The Ecclesia votes Yes/No/Abstain; highest Yes power that meets threshold
   seats the candidate (multi-candidate races may use separate proposals per
   seat in v1).
3. Term length is a governance parameter (default: **90 days** wall-clock
   equivalent in slots once the runtime is live).

## Article VII — Impeachment & vacancy

1. RankImpeachment may target any seated officer.
2. Passage removes the seat immediately; a successor election SHOULD follow.
3. Vacancy mid-term: Boule may appoint a caretaker for Bouleutes/Tamias until
   the next Ecclesia election; Archon vacancies require Ecclesia election.

## Article VIII — Amending this Constitution

1. Only a `ConstitutionAmendment` proposal may change articles or the
   in-protocol constitution id/hash.
2. Heightened Ecclesia thresholds apply (default: quorum **50%**, pass
   **66.7%** Yes among non-Abstain).
3. Archon Basileus assent **or** 2-of-3 ArchonCollegium is required before
   execution (Article IV).
4. The enacted text’s content hash is stored as `constitution_hash` so nodes
   can verify they share the same higher law.

## Article IX — Scope & non-goals (v1)

1. L1 civic governance binds **TLT** holders. L2 OVL sequencers and L3 DRC
   attestors remain **bonded operator sets**, not Ecclesia ranks (see
   `TOKEN_ROLES.md`).
2. This charter does not replace client-software rough consensus for
   non-enactable social upgrades.
3. Full node persistence, RPC, and wallet UX are layered on after the
   in-crate engine; until then the constitution and engine define the
   normative rules.

## Article X — Enactment

Constitution v1 is enacted when governance state records
`constitution_id = "constitution-v1"` and matching `constitution_hash`.
Subsequent amendments bump the id (`constitution-v2`, …) per Article VIII.

---

*Canonical machine-readable companion: `agora_governance::constitution`.*
