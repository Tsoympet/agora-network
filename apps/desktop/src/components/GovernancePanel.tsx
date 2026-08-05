import { FormEvent, useEffect, useState, type CSSProperties } from "react";
import type {
  LightClient,
  LightConstitution,
  LightProposal,
  VoteChoice,
} from "../../../shared/light-client";

const fieldStyle: CSSProperties = {
  width: "100%",
  padding: "0.65rem 0.75rem",
  border: "1px solid var(--agora-line)",
  background: "transparent",
  color: "var(--agora-ink)",
  fontFamily: "ui-monospace, monospace",
  fontSize: "0.85rem",
};

const btnStyle: CSSProperties = {
  padding: "0.55rem 1rem",
  border: "1px solid var(--agora-gold)",
  background: "transparent",
  color: "var(--agora-gold)",
  cursor: "pointer",
  fontFamily: "inherit",
  fontSize: "0.85rem",
};

type Props = {
  client: LightClient;
  /** Bech32 address used as voter / author when unlocked. */
  voterAddress: string | null;
  balance: number | null;
};

export function GovernancePanel({ client, voterAddress, balance }: Props) {
  const [constitution, setConstitution] = useState<LightConstitution | null>(null);
  const [proposals, setProposals] = useState<LightProposal[]>([]);
  const [eligible, setEligible] = useState(1);
  const [error, setError] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [proposalId, setProposalId] = useState("");
  const [choice, setChoice] = useState<VoteChoice>("yes");
  const [title, setTitle] = useState("");
  const [summary, setSummary] = useState("");

  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;

    const tick = async () => {
      try {
        const [c, g, list] = await Promise.all([
          client.getConstitution(),
          client.getGovernance(),
          client.listProposals(24),
        ]);
        if (cancelled) return;
        setConstitution(c);
        setEligible(g.ecclesia_eligible_power || 1);
        setProposals(list.proposals);
        setError(null);
      } catch (err) {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : "governance RPC offline");
      } finally {
        if (!cancelled) timer = window.setTimeout(tick, 4000);
      }
    };

    void tick();
    return () => {
      cancelled = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [client]);

  async function onVote(e: FormEvent) {
    e.preventDefault();
    if (!voterAddress) {
      setMsg("Unlock wallet / derive address to vote");
      return;
    }
    const id = Number(proposalId);
    if (!Number.isFinite(id) || id < 1) {
      setMsg("Enter a valid proposal id");
      return;
    }
    setBusy(true);
    setMsg(null);
    try {
      await client.castGovVote({
        id,
        voter: voterAddress,
        choice,
        raw_balance: balance ?? 0,
        total_supply: eligible,
      });
      setMsg(`Voted ${choice} on #${id}`);
      const list = await client.listProposals(24);
      setProposals(list.proposals);
    } catch (err) {
      setMsg(err instanceof Error ? err.message : "vote failed");
    } finally {
      setBusy(false);
    }
  }

  async function onSubmitSignal(e: FormEvent) {
    e.preventDefault();
    if (!voterAddress) {
      setMsg("Unlock wallet / derive address to propose");
      return;
    }
    if (!title.trim() || !summary.trim()) {
      setMsg("Title and summary required");
      return;
    }
    setBusy(true);
    setMsg(null);
    try {
      const res = await client.submitProposal({
        author: voterAddress,
        title: title.trim(),
        summary: summary.trim(),
        kind: { type: "text_signal" },
        slot: Date.now(),
      });
      setMsg(`Submitted proposal #${res.proposal_id} (deposit still required)`);
      setTitle("");
      setSummary("");
      const list = await client.listProposals(24);
      setProposals(list.proposals);
    } catch (err) {
      setMsg(err instanceof Error ? err.message : "submit failed");
    } finally {
      setBusy(false);
    }
  }

  async function onAck() {
    if (!voterAddress) {
      setMsg("Unlock wallet / derive address to acknowledge");
      return;
    }
    setBusy(true);
    setMsg(null);
    try {
      const res = await client.ackConstitution(voterAddress, Date.now());
      setMsg(`Acked ${res.constitution_id}`);
    } catch (err) {
      setMsg(err instanceof Error ? err.message : "ack failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section style={{ marginTop: "2.5rem", maxWidth: 520 }}>
      <p className="agora-eyebrow">Ecclesia</p>
      <h2 style={{ margin: "0.35rem 0 0.5rem", fontSize: "1.35rem" }}>Ballot</h2>
      <p style={{ margin: 0, fontSize: "0.9rem", color: "var(--agora-mist, #9aa3b2)" }}>
        Vote with quadratic TLT weight · acknowledge the constitution
      </p>

      {error ? (
        <p style={{ marginTop: "0.75rem", color: "var(--agora-danger)", fontSize: "0.85rem" }}>
          {error}
        </p>
      ) : null}

      {constitution ? (
        <div style={{ marginTop: "1rem", fontFamily: "ui-monospace, monospace", fontSize: "0.75rem" }}>
          <div>
            <span style={{ color: "var(--agora-mist, #9aa3b2)" }}>id </span>
            {constitution.id}
          </div>
          <div style={{ wordBreak: "break-all", color: "var(--agora-gold)" }}>
            {constitution.content_hash}
          </div>
          <button type="button" onClick={onAck} disabled={busy} style={{ ...btnStyle, marginTop: "0.75rem" }}>
            Acknowledge constitution
          </button>
        </div>
      ) : null}

      <ul style={{ listStyle: "none", padding: 0, margin: "1.25rem 0 0" }}>
        {proposals.length === 0 ? (
          <li style={{ fontSize: "0.85rem", color: "var(--agora-mist, #9aa3b2)" }}>
            No proposals yet
          </li>
        ) : (
          proposals.map((p) => (
            <li
              key={p.id}
              style={{
                borderBottom: "1px solid var(--agora-line)",
                padding: "0.65rem 0",
                fontSize: "0.85rem",
              }}
            >
              <strong>
                #{p.id} {p.title}
              </strong>{" "}
              <span style={{ color: "var(--agora-cyan)", fontFamily: "ui-monospace, monospace", fontSize: "0.75rem" }}>
                {p.status}
              </span>
              <div style={{ color: "var(--agora-mist, #9aa3b2)", marginTop: "0.25rem" }}>
                {p.summary}
              </div>
              <div style={{ fontFamily: "ui-monospace, monospace", fontSize: "0.7rem", marginTop: "0.35rem" }}>
                Y{p.tally.yes} N{p.tally.no} A{p.tally.abstain} V{p.tally.no_with_veto}
              </div>
            </li>
          ))
        )}
      </ul>

      <form onSubmit={onVote} style={{ marginTop: "1.25rem", display: "grid", gap: "0.65rem" }}>
        <p className="agora-eyebrow" style={{ margin: 0 }}>
          Cast vote
        </p>
        <input
          value={proposalId}
          onChange={(e) => setProposalId(e.target.value)}
          placeholder="proposal id"
          aria-label="Proposal id"
          style={fieldStyle}
        />
        <select
          value={choice}
          onChange={(e) => setChoice(e.target.value as VoteChoice)}
          aria-label="Vote choice"
          style={fieldStyle}
        >
          <option value="yes">yes</option>
          <option value="no">no</option>
          <option value="abstain">abstain</option>
          <option value="no_with_veto">no_with_veto</option>
        </select>
        <button type="submit" disabled={busy} style={btnStyle}>
          {busy ? "…" : "Cast vote"}
        </button>
      </form>

      <form onSubmit={onSubmitSignal} style={{ marginTop: "1.5rem", display: "grid", gap: "0.65rem" }}>
        <p className="agora-eyebrow" style={{ margin: 0 }}>
          Text signal
        </p>
        <input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="title"
          aria-label="Proposal title"
          style={fieldStyle}
        />
        <textarea
          value={summary}
          onChange={(e) => setSummary(e.target.value)}
          placeholder="summary"
          aria-label="Proposal summary"
          rows={3}
          style={{ ...fieldStyle, resize: "vertical" }}
        />
        <button type="submit" disabled={busy} style={btnStyle}>
          Submit proposal
        </button>
      </form>

      {msg ? (
        <p style={{ marginTop: "0.75rem", fontSize: "0.85rem", color: "var(--agora-cyan)" }}>
          {msg}
        </p>
      ) : null}
      <p style={{ marginTop: "0.5rem", fontSize: "0.75rem", color: "var(--agora-mist, #9aa3b2)" }}>
        Mutating calls need <code>AGORA_RPC_TOKEN</code> when the node enforces auth.
      </p>
    </section>
  );
}
