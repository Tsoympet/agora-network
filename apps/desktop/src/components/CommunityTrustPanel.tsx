import { useEffect, useMemo, useState, type CSSProperties } from "react";
import {
  communityTrustView,
  defaultTreasuryInputs,
  type CommunityProposalInput,
  type EcosystemCounts,
  type HubInput,
  type TreasuryInputs,
} from "../../../shared/community-trust";

type CommunityReader = {
  hubs: () => Promise<{ data: HubInput[] | null; label: string }>;
  proposals: () => Promise<{ data: CommunityProposalInput[] | null; label: string }>;
  ecosystem: () => Promise<{ data: EcosystemCounts | null; label: string }>;
};

const fieldStyle: CSSProperties = {
  width: "100%",
  padding: "0.65rem 0.75rem",
  borderRadius: 8,
  border: "1px solid rgba(197, 152, 53, 0.35)",
  background: "rgba(16, 18, 24, 0.65)",
  color: "var(--agora-ink)",
  fontFamily: "ui-monospace, monospace",
  fontSize: "0.85rem",
};

const btnStyle: CSSProperties = {
  padding: "0.55rem 0.9rem",
  borderRadius: 8,
  border: "1px solid rgba(6, 187, 223, 0.45)",
  background: "transparent",
  color: "var(--agora-cyan)",
  fontFamily: "var(--font-ui)",
  cursor: "pointer",
};

const badgeStyle = (onChain: boolean): CSSProperties => ({
  display: "inline-block",
  padding: "0.15rem 0.45rem",
  border: `1px solid ${onChain ? "var(--agora-cyan)" : "var(--agora-gold)"}`,
  color: onChain ? "var(--agora-cyan)" : "var(--agora-gold)",
  letterSpacing: "0.08em",
  fontSize: "0.72rem",
});

export function CommunityTrustPanel({
  subjectAddress,
  community,
}: {
  subjectAddress: string | null;
  community?: CommunityReader | null;
}) {
  const [query, setQuery] = useState("");
  const [language, setLanguage] = useState("en");
  const [interests, setInterests] = useState("");
  const [treasury, setTreasury] = useState<TreasuryInputs>(defaultTreasuryInputs());
  const [fundingNote, setFundingNote] = useState<string | null>(null);
  const [hubs, setHubs] = useState<HubInput[]>([]);
  const [serviceProposals, setServiceProposals] = useState<CommunityProposalInput[]>([]);
  const [ecosystem, setEcosystem] = useState<EcosystemCounts | null>(null);
  const [serviceLabel, setServiceLabel] = useState<string | null>(null);
  useEffect(() => {
    if (!community) return;
    let cancelled = false;
    void Promise.all([community.hubs(), community.proposals(), community.ecosystem()]).then(
      ([hubView, proposalView, ecosystemView]) => {
        if (cancelled) return;
        setHubs(hubView.data ?? []);
        setServiceProposals(proposalView.data ?? []);
        setEcosystem(ecosystemView.data);
        setServiceLabel(hubView.label);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [community]);
  const view = useMemo(
    () =>
      communityTrustView({
        query,
        treasury,
        subjectAddress,
        language,
        interestFilters: interests
          .split(",")
          .map((part) => part.trim())
          .filter(Boolean),
        hubs,
        proposals: serviceProposals,
        ecosystem,
      }),
    [ecosystem, hubs, interests, language, query, serviceProposals, subjectAddress, treasury],
  );

  function setTreasuryField(key: keyof TreasuryInputs, value: string) {
    setTreasury((current) => ({ ...current, [key]: value }));
  }

  return (
    <section className="agora-rise agora-rise-delay-3" style={{ marginTop: "2.75rem", maxWidth: 720 }}>
      <p className="agora-eyebrow">Community trust</p>
      <p style={{ marginTop: "0.55rem", color: "var(--agora-ink-muted)", fontSize: "0.9rem" }}>
        Scaffold. Keys stay on this device. Seed records are local data.
      </p>
      <p style={{ marginTop: "0.45rem", color: "var(--agora-ink)", fontSize: "0.9rem" }}>{view.notice}</p>
      {serviceLabel ? <p style={{ color: "var(--agora-ink-muted)", fontSize: "0.85rem" }}>{serviceLabel}</p> : null}

      <label style={{ display: "block", marginTop: "1rem" }}>
        <span style={{ fontSize: "0.8rem", color: "var(--agora-gold)" }}>Region search</span>
        <input
          aria-label="Region search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Greece, Europe, Asia, Americas, Athens"
          style={{ ...fieldStyle, marginTop: "0.35rem" }}
        />
      </label>
      <ul style={{ marginTop: "0.75rem", padding: 0, listStyle: "none", display: "grid", gap: "0.55rem" }}>
        {view.communities.map((community) => (
          <li key={community.id} style={{ borderTop: "1px solid rgba(197, 152, 53, 0.25)", paddingTop: "0.45rem" }}>
            <strong>{community.name}</strong>
            <div style={{ color: "var(--agora-ink-muted)", fontSize: "0.85rem" }}>
              {[community.country, community.region, community.city].filter(Boolean).join(" · ") || "Attribute community"}
            </div>
            <div style={{ fontSize: "0.8rem" }}>
              {community.languages.join(", ")} · {community.interests.join(", ")} · {community.specializations.join(", ")}
            </div>
          </li>
        ))}
      </ul>

      <p className="agora-eyebrow" style={{ marginTop: "1.5rem" }}>Proposals</p>
      <p style={{ color: "var(--agora-ink-muted)", fontSize: "0.85rem" }}>
        Scaffold cards. An ON-CHAIN badge is a consensus record. An ADVISORY badge is a public note.
      </p>
      {view.proposals.map((proposal) => (
        <article key={proposal.id} style={{ marginTop: "0.85rem" }}>
          <span style={badgeStyle(proposal.badge === "ON-CHAIN")}>{proposal.badge}</span>
          <h3 style={{ fontFamily: "var(--font-display)", fontSize: "1.05rem", margin: "0.4rem 0" }}>{proposal.id}</h3>
          <p>Creator: {proposal.creator}</p>
          <p>Why: {proposal.why}</p>
          <p>What changes: {proposal.what_changes}</p>
          <p>Expected cost: {proposal.expected_cost}</p>
          <p>Treasury impact: {proposal.treasury_impact}</p>
          <p>
            Voting period: slots {proposal.voting_period.start_slot}–{proposal.voting_period.end_slot}
          </p>
          <p>Eligibility: {proposal.eligibility}</p>
          <p>
            Current votes: yes {proposal.current_votes.yes}, no {proposal.current_votes.no}, abstain{" "}
            {proposal.current_votes.abstain}, veto {proposal.current_votes.no_with_veto}
          </p>
          <p>Final result: {proposal.final_result}</p>
          <p>Implementation: {proposal.implementation_status}</p>
          <p>Commits: {proposal.commit_links.length ? proposal.commit_links.join(", ") : "none yet"}</p>
          <p>Releases: {proposal.release_links.length ? proposal.release_links.join(", ") : "none yet"}</p>
        </article>
      ))}
      {view.serviceProposals.map((proposal) => (
        <article key={proposal.id} style={{ marginTop: "0.85rem" }}>
          <span style={badgeStyle(proposal.badge === "ON-CHAIN")}>{proposal.badge}</span>
          <h3 style={{ fontFamily: "var(--font-display)", fontSize: "1.05rem", margin: "0.4rem 0" }}>{proposal.id}</h3>
          <p>{proposal.note}</p>
          {proposal.lines.map((line) => (
            <p key={line.label}>
              {line.label}: {line.value}
            </p>
          ))}
        </article>
      ))}

      <p className="agora-eyebrow" style={{ marginTop: "1.5rem" }}>Rewards</p>
      <p style={{ color: "var(--agora-ink-muted)", fontSize: "0.85rem" }}>
        Amounts are already-issued base units. TLT emission stays blocked. Nothing here submits a transaction.
      </p>
      <div style={{ display: "grid", gap: "0.45rem", marginTop: "0.6rem" }}>
        {(
          [
            ["drc_available", "DRC treasury"],
            ["drc_amount", "DRC amount"],
            ["ovl_available", "OVL treasury"],
            ["ovl_amount", "OVL amount"],
            ["tlt_available", "TLT treasury"],
            ["tlt_amount", "TLT amount"],
          ] as const
        ).map(([key, label]) => (
          <label key={key}>
            <span style={{ fontSize: "0.75rem", color: "var(--agora-ink-muted)" }}>{label}</span>
            <input
              aria-label={label}
              value={treasury[key]}
              onChange={(event) => setTreasuryField(key, event.target.value)}
              style={fieldStyle}
            />
          </label>
        ))}
      </div>
      <ul style={{ marginTop: "0.8rem", padding: 0, listStyle: "none", display: "grid", gap: "0.45rem" }}>
        {view.rewards.map((row) => (
          <li key={row.label} style={{ display: "grid", gap: "0.2rem" }}>
            <div>
              {row.label} · {row.ui_blocked ? "blocked" : "funding path"} · supply delta {row.supply_delta_base_units}
            </div>
            <button
              type="button"
              disabled={row.control.disabled}
              style={{ ...btnStyle, opacity: row.control.disabled ? 0.45 : 1 }}
              onClick={() => setFundingNote(row.funding_note)}
            >
              {row.control.label}
            </button>
            {row.block_reason ? (
              <span style={{ color: "var(--agora-ink-muted)", fontSize: "0.8rem" }}>{row.block_reason}</span>
            ) : null}
          </li>
        ))}
      </ul>
      {fundingNote ? <p>{fundingNote}</p> : null}

      <p className="agora-eyebrow" style={{ marginTop: "1.5rem" }}>Public analytics</p>
      <p style={{ color: "var(--agora-ink-muted)", fontSize: "0.85rem" }}>{view.analytics.assumption}</p>
      <ul style={{ padding: 0, listStyle: "none", display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.35rem" }}>
        {view.analyticsRows.map((entry) => (
          <li key={entry.key}>
            {view.analyticsLabels[entry.key]}: {entry.counted ? entry.value : "not counted"}
          </li>
        ))}
      </ul>

      <p className="agora-eyebrow" style={{ marginTop: "1.5rem" }}>Portable identity</p>
      <label style={{ display: "block" }}>
        <span style={{ fontSize: "0.8rem", color: "var(--agora-gold)" }}>Language preference</span>
        <input aria-label="Language preference" value={language} onChange={(event) => setLanguage(event.target.value)} style={{ ...fieldStyle, marginTop: "0.35rem" }} />
      </label>
      <label style={{ display: "block", marginTop: "0.5rem" }}>
        <span style={{ fontSize: "0.8rem", color: "var(--agora-gold)" }}>Interest filters</span>
        <input
          aria-label="Interest filters"
          value={interests}
          onChange={(event) => setInterests(event.target.value)}
          placeholder="governance, payments"
          style={{ ...fieldStyle, marginTop: "0.35rem" }}
        />
      </label>
      <p style={{ fontSize: "0.85rem" }}>
        Public subject: {subjectAddress ?? "none selected"}. Local preferences are included when you copy this JSON.
        Private keys are omitted.
      </p>
      {"error" in view.identity ? (
        <p style={{ color: "#d65a5a" }}>{view.identity.error}</p>
      ) : (
        <pre style={{ whiteSpace: "pre-wrap", fontSize: "0.75rem", color: "var(--agora-ink-muted)" }}>{view.identity.json}</pre>
      )}

      <p className="agora-eyebrow" style={{ marginTop: "1.5rem" }}>Trust</p>
      <ul style={{ padding: 0, listStyle: "none", display: "grid", gap: "0.85rem" }}>
        {view.assumptions.map((row) => (
          <li key={row.id}>
            <strong>{row.title}</strong> · {row.posture}
            <div style={{ fontSize: "0.85rem" }}>{row.checks}</div>
            <div style={{ fontSize: "0.85rem", color: "var(--agora-gold)" }}>{row.assumption}</div>
          </li>
        ))}
      </ul>
    </section>
  );
}
