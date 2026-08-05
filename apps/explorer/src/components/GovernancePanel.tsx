import { useEffect, useState } from "react";
import type {
  LightConstitution,
  LightGovernance,
  LightOffice,
  LightProposal,
  RpcStatus,
} from "../../../shared/light-client";
import { getClient, rpcUrl } from "../lib/rpc";

const POLL_MS = Number(import.meta.env.VITE_AGORA_POLL_MS) || 4000;

export function GovernancePanel() {
  const [constitution, setConstitution] = useState<LightConstitution | null>(null);
  const [gov, setGov] = useState<LightGovernance | null>(null);
  const [proposals, setProposals] = useState<LightProposal[]>([]);
  const [offices, setOffices] = useState<LightOffice[]>([]);
  const [status, setStatus] = useState<RpcStatus>("idle");
  const [error, setError] = useState<string | null>(null);
  const [showBody, setShowBody] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;
    const client = getClient();

    const tick = async () => {
      try {
        const [c, g, list, o] = await Promise.all([
          client.getConstitution(),
          client.getGovernance(),
          client.listProposals(32),
          client.listOffices(),
        ]);
        if (cancelled) return;
        setConstitution(c);
        setGov(g);
        setProposals(list.proposals);
        setOffices(o.offices);
        setStatus("ok");
        setError(null);
      } catch (err) {
        if (cancelled) return;
        setStatus("error");
        setError(err instanceof Error ? err.message : "RPC unreachable");
      } finally {
        if (!cancelled) timer = window.setTimeout(tick, POLL_MS);
      }
    };

    void tick();
    return () => {
      cancelled = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, []);

  const seated = offices.filter((o) => o.holder);

  return (
    <div className="relative">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="agora-eyebrow">Ecclesia</p>
          <h2 className="agora-display mt-3 text-3xl md:text-4xl">Civic ballot</h2>
          <p className="agora-lede mt-4">
            Constitution, elected ranks, and open proposals —{" "}
            <code className="text-[var(--agora-ink)]">agora_getGovernance</code>{" "}
            / <code className="text-[var(--agora-ink)]">agora_listProposals</code>.
          </p>
        </div>
        <p className="text-sm text-mist">
          <span
            className={
              status === "ok"
                ? "text-[var(--agora-cyan)]"
                : status === "error"
                  ? "text-[var(--agora-danger)]"
                  : "text-mist"
            }
          >
            {status === "ok" ? "connected" : status === "error" ? "offline" : "connecting"}
          </span>
          {" · "}
          <span className="font-mono text-xs opacity-80">{rpcUrl()}</span>
        </p>
      </div>

      {error ? (
        <p className="mt-6 text-sm text-[var(--agora-danger)]">{error}</p>
      ) : null}

      {constitution ? (
        <div className="mt-10 border-t border-[var(--agora-line)] pt-8">
          <p className="agora-eyebrow">Constitution</p>
          <dl className="mt-4 grid gap-3 font-mono text-xs md:grid-cols-2 md:text-sm">
            <div>
              <dt className="text-mist">id</dt>
              <dd className="text-[var(--agora-ink)]">{constitution.id}</dd>
            </div>
            <div>
              <dt className="text-mist">content_hash</dt>
              <dd className="break-all text-[var(--agora-gold)]">{constitution.content_hash}</dd>
            </div>
            {gov ? (
              <div>
                <dt className="text-mist">eligible power</dt>
                <dd>{gov.ecclesia_eligible_power}</dd>
              </div>
            ) : null}
            {gov ? (
              <div>
                <dt className="text-mist">acks / topics</dt>
                <dd>
                  {gov.constitution_ack_count} / {gov.topic_count}
                </dd>
              </div>
            ) : null}
          </dl>
          <button
            type="button"
            className="agora-btn agora-btn-ghost mt-4 text-sm"
            onClick={() => setShowBody((v) => !v)}
          >
            {showBody ? "Hide body" : "Show body"}
          </button>
          {showBody ? (
            <pre className="mt-4 max-h-64 overflow-auto whitespace-pre-wrap border border-[var(--agora-line)] bg-[var(--agora-obsidian)]/40 p-4 font-mono text-xs text-mist">
              {constitution.body_markdown}
            </pre>
          ) : null}
        </div>
      ) : null}

      <div className="mt-12 border-t border-[var(--agora-line)] pt-8">
        <p className="agora-eyebrow">Offices</p>
        <h3 className="agora-display mt-2 text-2xl">Seated ranks</h3>
        {seated.length === 0 ? (
          <p className="mt-4 text-sm text-mist">No seats filled yet — open a RankElection.</p>
        ) : (
          <ul className="mt-6 space-y-3 font-mono text-xs md:text-sm">
            {seated.map((o) => (
              <li
                key={`${o.rank}-${o.seat_index}`}
                className="flex flex-wrap items-baseline justify-between gap-2 border-b border-[var(--agora-line)] pb-2"
              >
                <span className="text-[var(--agora-gold)]">
                  {o.title}
                  {o.seat_index > 0 ? ` #${o.seat_index}` : ""}
                </span>
                <span className="text-mist">{o.greek}</span>
                <span className="w-full break-all text-[var(--agora-ink)] md:w-auto">
                  {o.holder}
                </span>
              </li>
            ))}
          </ul>
        )}
        <p className="mt-4 text-xs text-mist">{offices.length} seats total</p>
      </div>

      <div className="mt-12 border-t border-[var(--agora-line)] pt-8">
        <p className="agora-eyebrow">Proposals</p>
        <h3 className="agora-display mt-2 text-2xl">Open ballot board</h3>
        {proposals.length === 0 ? (
          <p className="mt-4 text-sm text-mist">No proposals yet.</p>
        ) : (
          <ul className="mt-6 space-y-6">
            {proposals.map((p) => (
              <li key={p.id} className="border-b border-[var(--agora-line)] pb-4">
                <div className="flex flex-wrap items-baseline justify-between gap-2">
                  <h4 className="text-lg text-[var(--agora-ink)]">
                    #{p.id} {p.title}
                  </h4>
                  <span className="font-mono text-xs text-[var(--agora-cyan)]">
                    {p.status} · {p.chamber}
                  </span>
                </div>
                <p className="mt-2 text-sm text-mist">{p.summary}</p>
                <p className="mt-3 font-mono text-xs text-mist">
                  yes {p.tally.yes} · no {p.tally.no} · abstain {p.tally.abstain} ·
                  veto {p.tally.no_with_veto} · deposit {p.deposit}
                </p>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
