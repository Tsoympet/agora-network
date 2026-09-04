import { useEffect, useState } from "react";
import type {
  LightAmount,
  LightCommunityRegistry,
  LightFinality,
  LightProtocolTreasuries,
  LightRewardPool,
  LightValidatorSet,
  NativeAssetTicker,
  RpcStatus,
} from "../../../shared/light-client";
import { getClient, rpcUrl } from "../lib/rpc";

const POLL_MS = Number(import.meta.env.VITE_AGORA_POLL_MS) || 5000;

const ASSETS: Array<{
  ticker: NativeAssetTicker;
  name: string;
  role: string;
  mechanism: string;
}> = [
  {
    ticker: "TLT",
    name: "Talanton",
    role: "Ordering · settlement",
    mechanism: "RandomX · mineable",
  },
  {
    ticker: "OVL",
    name: "Ovolos",
    role: "Execution · builders",
    mechanism: "PoS · never mined",
  },
  {
    ticker: "DRC",
    name: "Drachma",
    role: "Payments · community",
    mechanism: "PoS · never mined",
  },
];

type Snapshot = {
  tip: string;
  finality: LightFinality | null;
  validators: Partial<Record<"OVL" | "DRC", LightValidatorSet>>;
  rewards: Partial<Record<"OVL" | "DRC", LightRewardPool>>;
  treasuries: LightProtocolTreasuries | null;
  community: LightCommunityRegistry | null;
};

function compactHash(value?: string): string {
  if (!value) return "—";
  return value.length > 18
    ? `${value.slice(0, 10)}…${value.slice(-6)}`
    : value;
}

function formatAmount(value?: LightAmount): string {
  if (value === undefined) return "—";
  const raw = String(value);
  return raw.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

function ratio(signed?: LightAmount, active?: LightAmount): string {
  if (signed === undefined || active === undefined) return "awaiting";
  const signedNumber = Number(signed);
  const activeNumber = Number(active);
  if (!Number.isFinite(signedNumber) || !Number.isFinite(activeNumber) || activeNumber <= 0) {
    return "no active stake";
  }
  return `${((signedNumber / activeNumber) * 100).toFixed(1)}%`;
}

function statusClass(met: boolean): string {
  return met ? "text-[var(--agora-cyan)]" : "text-mist";
}

export function TridentOverview() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [status, setStatus] = useState<RpcStatus>("idle");
  const [unavailable, setUnavailable] = useState(0);

  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;
    const client = getClient();

    const tick = async () => {
      try {
        const tips = await client.getDagTips();
        const tip = tips[0] ?? "";
        const requests = await Promise.allSettled([
          tip ? client.getFinality(tip) : Promise.resolve(null),
          client.getValidatorSet("OVL"),
          client.getValidatorSet("DRC"),
          client.getRewardPool("OVL"),
          client.getRewardPool("DRC"),
          client.getProtocolTreasuries(),
          client.getCommunityRegistry(24),
        ] as const);
        if (cancelled) return;

        const value = <T,>(index: number): T | null => {
          const result = requests[index];
          return result.status === "fulfilled" ? (result.value as T) : null;
        };
        setSnapshot({
          tip,
          finality: value<LightFinality>(0),
          validators: {
            OVL: value<LightValidatorSet>(1) ?? undefined,
            DRC: value<LightValidatorSet>(2) ?? undefined,
          },
          rewards: {
            OVL: value<LightRewardPool>(3) ?? undefined,
            DRC: value<LightRewardPool>(4) ?? undefined,
          },
          treasuries: value<LightProtocolTreasuries>(5),
          community: value<LightCommunityRegistry>(6),
        });
        const failed = requests.filter((result) => result.status === "rejected").length;
        setUnavailable(failed);
        setStatus(failed === requests.length ? "error" : "ok");
      } catch {
        if (!cancelled) setStatus("error");
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

  const finality = snapshot?.finality;
  const ovlMet =
    finality?.ovl_signed_stake !== undefined &&
    finality?.ovl_active_stake !== undefined &&
    Number(finality.ovl_signed_stake) * 3 >= Number(finality.ovl_active_stake) * 2;
  const drcMet =
    finality?.drc_signed_stake !== undefined &&
    finality?.drc_active_stake !== undefined &&
    Number(finality.drc_signed_stake) * 3 >= Number(finality.drc_active_stake) * 2;

  return (
    <div className="relative">
      <div className="flex flex-wrap items-end justify-between gap-5">
        <div>
          <p className="agora-eyebrow">Trident L1</p>
          <h2 className="agora-display mt-3 text-3xl md:text-5xl">
            One ledger. Three native assets.
          </h2>
          <p className="agora-lede mt-4">
            TLT orders the BlockDAG. Independent OVL and DRC validator quorums
            finalize the same canonical state.
          </p>
        </div>
        <div className="text-right font-mono text-xs text-mist">
          <p className={status === "ok" ? "text-[var(--agora-cyan)]" : "text-mist"}>
            {status === "ok"
              ? unavailable > 0
                ? `partial · ${unavailable} unavailable`
                : "read RPC connected"
              : status === "error"
                ? "Trident RPC unavailable"
                : "connecting"}
          </p>
          <p className="mt-1">{rpcUrl()}</p>
        </div>
      </div>

      <div className="mt-12 grid overflow-hidden border border-[var(--agora-line)] lg:grid-cols-3">
        {ASSETS.map((asset, index) => {
          const treasury = snapshot?.treasuries?.treasuries.find(
            (entry) => entry.asset === asset.ticker,
          );
          const validator =
            asset.ticker === "TLT" ? undefined : snapshot?.validators[asset.ticker];
          const reward =
            asset.ticker === "TLT" ? undefined : snapshot?.rewards[asset.ticker];
          return (
            <article
              key={asset.ticker}
              className={`relative min-h-64 p-7 ${
                index > 0 ? "border-t border-[var(--agora-line)] lg:border-l lg:border-t-0" : ""
              }`}
            >
              <div className="flex items-start justify-between">
                <div>
                  <p className="font-mono text-xs tracking-[0.2em] text-[var(--agora-cyan)]">
                    0x0{index}
                  </p>
                  <h3 className="agora-brand mt-3 text-3xl">{asset.ticker}</h3>
                  <p className="mt-1 text-sm text-[var(--agora-ink)]">{asset.name}</p>
                </div>
                <span className="border border-[var(--agora-line)] px-2 py-1 font-mono text-[0.65rem] uppercase tracking-widest text-mist">
                  native
                </span>
              </div>
              <p className="mt-8 text-sm text-mist">{asset.role}</p>
              <p className="mt-1 font-mono text-xs text-[var(--agora-gold)]">
                {asset.mechanism}
              </p>
              <dl className="mt-7 grid grid-cols-2 gap-4 border-t border-[var(--agora-line)] pt-5 font-mono text-xs">
                <div>
                  <dt className="text-mist">active stake</dt>
                  <dd className="mt-1 text-[var(--agora-ink)]">
                    {asset.ticker === "TLT"
                      ? "PoW"
                      : formatAmount(validator?.total_active_stake)}
                  </dd>
                </div>
                <div>
                  <dt className="text-mist">
                    {asset.ticker === "TLT" ? "treasury" : "reward pool"}
                  </dt>
                  <dd className="mt-1 text-[var(--agora-ink)]">
                    {formatAmount(
                      asset.ticker === "TLT" ? treasury?.balance : reward?.amount,
                    )}
                  </dd>
                </div>
              </dl>
            </article>
          );
        })}
      </div>

      <div className="mt-16 grid gap-12 lg:grid-cols-[1.4fr_1fr]">
        <section>
          <div className="flex items-baseline justify-between gap-4">
            <div>
              <p className="agora-eyebrow">Finality certificate</p>
              <h3 className="agora-display mt-2 text-2xl">Triple conjunction</h3>
            </div>
            <span
              className={`font-mono text-xs uppercase tracking-widest ${
                finality?.finalized
                  ? "text-[var(--agora-cyan)]"
                  : "text-[var(--agora-gold)]"
              }`}
            >
              {finality?.finalized ? "finalized" : finality?.state ?? "awaiting"}
            </span>
          </div>
          <div className="mt-7 grid grid-cols-[1fr_auto_1fr_auto_1fr] items-center gap-2">
            <FinalityGate
              label="TLT work"
              value={finality?.pow_work_met ? "threshold met" : "pending"}
              met={Boolean(finality?.pow_work_met)}
            />
            <span className="text-mist">∧</span>
            <FinalityGate
              label="OVL stake"
              value={ratio(finality?.ovl_signed_stake, finality?.ovl_active_stake)}
              met={ovlMet}
            />
            <span className="text-mist">∧</span>
            <FinalityGate
              label="DRC stake"
              value={ratio(finality?.drc_signed_stake, finality?.drc_active_stake)}
              met={drcMet}
            />
          </div>
          <dl className="mt-7 border-t border-[var(--agora-line)] pt-5 font-mono text-xs text-mist">
            <div className="flex flex-wrap justify-between gap-2">
              <dt>checkpoint</dt>
              <dd className="text-[var(--agora-ink)]" title={snapshot?.tip}>
                {compactHash(snapshot?.tip)}
              </dd>
            </div>
            <div className="mt-2 flex justify-between gap-2">
              <dt>finalized blue score</dt>
              <dd className="text-[var(--agora-ink)]">
                {finality?.finalized_tip_blue_score ?? "—"}
              </dd>
            </div>
          </dl>
        </section>

        <section>
          <p className="agora-eyebrow">Canonical community</p>
          <h3 className="agora-display mt-2 text-2xl">Public commitments</h3>
          <div className="mt-7 grid grid-cols-2 gap-px bg-[var(--agora-line)] border border-[var(--agora-line)]">
            {[
              ["Hubs", snapshot?.community?.counts.hubs],
              ["Passports", snapshot?.community?.counts.passport_attestations],
              ["Grants", snapshot?.community?.counts.grants],
              ["Missions", snapshot?.community?.counts.missions],
            ].map(([label, count]) => (
              <div key={String(label)} className="bg-[var(--agora-obsidian)] p-4">
                <p className="font-mono text-2xl text-[var(--agora-ink)]">
                  {count ?? "—"}
                </p>
                <p className="mt-1 text-xs uppercase tracking-widest text-mist">{label}</p>
              </div>
            ))}
          </div>
          <p className="mt-5 font-mono text-xs text-mist">
            registry root ·{" "}
            <span
              className="text-[var(--agora-gold)]"
              title={snapshot?.community?.root}
            >
              {compactHash(snapshot?.community?.root)}
            </span>
          </p>
          <p className="mt-3 text-xs text-mist">
            Maturity: {snapshot?.community?.maturity ?? "Scaffold"}. Read-only
            commitments do not imply active treasury or registry mutation.
          </p>
        </section>
      </div>
    </div>
  );
}

function FinalityGate({
  label,
  value,
  met,
}: {
  label: string;
  value: string;
  met: boolean;
}) {
  return (
    <div className="min-w-0 border-l border-[var(--agora-line)] pl-3">
      <p className="text-[0.65rem] uppercase tracking-widest text-mist">{label}</p>
      <p className={`mt-2 font-mono text-xs md:text-sm ${statusClass(met)}`}>
        {value}
      </p>
    </div>
  );
}
