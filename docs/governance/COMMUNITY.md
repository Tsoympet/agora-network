# Community board (EOS forum patterns)

Companion to the civic Constitution. Lives in `agora_governance::community`
and is persisted with governance in `meta/governance`.

## Borrowed from EOS

| Idea | Agora |
| --- | --- |
| `eosio.forum` topics | `ForumTopic` + `agora_postForumTopic` / `agora_listForumTopics` |
| Constitution acknowledgment | `agora_ackConstitution` binds address → constitution hash |
| Worker proposals | `TopicCategory::WorkerIdea` before formal `TreasurySpend` |

## Categories

`discussion`, `signal`, `worker_idea`, `assembly_notice`

Discussion is **not** a binding vote — only chamber ballots on formal
proposals count (see Constitution Art. IV).
