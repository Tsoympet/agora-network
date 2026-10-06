import { FormEvent, useEffect, useState } from "react";
import type {
  LightAccountBalances,
  LightDrcOfferLookup,
  LightTltCovenantLookup,
  RpcStatus,
} from "../../../shared/light-client";
import { getClient, rpcUrl } from "../lib/rpc";

const HEX64 = /^(0x)?[0-9a-fA-F]{64}$/;

function formatAmount(value: number | string | undefined): string {
  if (value === undefined) return "—";
  return String(value).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

export function ProtocolLanes() {
  const client = getClient();
  const [ethChain, setEthChain] = useState<string | null>(null);
  const [ethBlock, setEthBlock] = useState<string | null>(null);
  const [ethStatus, setEthStatus] = useState<RpcStatus>("idle");

  const [account, setAccount] = useState("");
  const [balances, setBalances] = useState<LightAccountBalances | null>(null);
  const [accountError, setAccountError] = useState<string | null>(null);
  const [accountBusy, setAccountBusy] = useState(false);

  const [laneId, setLaneId] = useState("");
  const [covenant, setCovenant] = useState<LightTltCovenantLookup | null>(null);
  const [offer, setOffer] = useState<LightDrcOfferLookup | null>(null);
  const [laneError, setLaneError] = useState<string | null>(null);
  const [laneBusy, setLaneBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    async function loadEth() {
      setEthStatus("idle");
      try {
        const [chainId, blockNumber] = await Promise.all([
          client.getEthChainId(),
          client.getEthBlockNumber(),
        ]);
        if (cancelled) return;
        setEthChain(String(chainId));
        setEthBlock(String(blockNumber));
        setEthStatus("ok");
      } catch {
        if (!cancelled) setEthStatus("error");
      }
    }
    void loadEth();
    return () => {
      cancelled = true;
    };
  }, [client]);

  async function onAccount(e: FormEvent) {
    e.preventDefault();
    setAccountBusy(true);
    setAccountError(null);
    setBalances(null);
    try {
      const next = await client.getAccountBalances(account.trim());
      setBalances(next);
    } catch (err) {
      setAccountError(err instanceof Error ? err.message : "account lookup failed");
    } finally {
      setAccountBusy(false);
    }
  }

  async function onLane(e: FormEvent) {
    e.preventDefault();
    const raw = laneId.trim();
    if (!HEX64.test(raw)) {
      setLaneError("Enter a 64-hex covenant or offer id.");
      setCovenant(null);
      setOffer(null);
      return;
    }
    setLaneBusy(true);
    setLaneError(null);
    setCovenant(null);
    setOffer(null);
    try {
      const [covenantLookup, offerLookup] = await Promise.all([
        client.getTltCovenant(raw),
        client.getDrcOffer(raw),
      ]);
      setCovenant(covenantLookup);
      setOffer(offerLookup);
    } catch (err) {
      setLaneError(err instanceof Error ? err.message : "lane lookup failed");
    } finally {
      setLaneBusy(false);
    }
  }

  return (
    <div className="relative">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="agora-eyebrow">Protocol lanes</p>
          <h2 className="agora-display mt-3 text-3xl md:text-4xl">
            TLT · DRC · OVL reads
          </h2>
          <p className="agora-lede mt-4">
            Experimental queries for native three-asset balances, TLT covenants,
            DRC offers, and canonical OVL Ethereum JSON-RPC. Compact gossip
            still uses the full body for every non-UTXO lane.
          </p>
        </div>
        <p className="font-mono text-xs text-mist opacity-80">{rpcUrl()}</p>
      </div>

      <div className="mt-10 grid gap-8 lg:grid-cols-2">
        <section>
          <p className="agora-eyebrow">OVL Ethereum RPC</p>
          <p className="mt-3 font-mono text-sm text-[var(--agora-ink)]">
            eth_chainId{" "}
            <span className="text-[var(--agora-cyan)]">{ethChain ?? "—"}</span>
            {" · "}
            eth_blockNumber{" "}
            <span className="text-[var(--agora-cyan)]">{ethBlock ?? "—"}</span>
          </p>
          <p className="mt-2 text-sm text-mist">
            {ethStatus === "ok"
              ? "Reads the stored OVL-EVM world. Raw envelopes stay a process-local inbox."
              : ethStatus === "error"
                ? "Ethereum RPC unavailable on this node."
                : "Querying…"}
          </p>
        </section>

        <form onSubmit={onAccount} className="flex min-w-0 flex-col gap-3">
          <p className="agora-eyebrow">Native balances</p>
          <label className="flex min-w-0 flex-col gap-2">
            <span className="text-sm text-mist">Account</span>
            <input
              type="text"
              spellCheck={false}
              autoComplete="off"
              placeholder="Bech32m or 40-hex"
              value={account}
              onChange={(e) => setAccount(e.target.value)}
              className="w-full border border-[var(--agora-line)] bg-[rgba(14,16,20,0.55)] px-4 py-3 font-mono text-sm text-[var(--agora-ink)] outline-none focus:border-[var(--agora-gold)]"
            />
          </label>
          <button
            type="submit"
            disabled={accountBusy || !account.trim()}
            className="agora-btn agora-btn-primary self-start disabled:opacity-50"
          >
            {accountBusy ? "Looking up…" : "agora_getAccountBalances"}
          </button>
          {accountError ? (
            <p className="text-sm text-[var(--agora-danger)]">{accountError}</p>
          ) : null}
          {balances ? (
            <dl className="space-y-2 font-mono text-sm">
              <div>
                <dt className="text-mist">TLT UTXO</dt>
                <dd>{formatAmount(balances.tlt.balance)}</dd>
              </div>
              <div>
                <dt className="text-mist">OVL account / nonce</dt>
                <dd>
                  {formatAmount(balances.ovl.balance)} / {balances.ovl.nonce}
                </dd>
              </div>
              <div>
                <dt className="text-mist">DRC account / nonce</dt>
                <dd>
                  {formatAmount(balances.drc.balance)} / {balances.drc.nonce}
                </dd>
              </div>
            </dl>
          ) : null}
        </form>
      </div>

      <form onSubmit={onLane} className="mt-10 flex flex-col gap-4 md:flex-row md:items-end">
        <label className="flex min-w-0 flex-1 flex-col gap-2">
          <span className="text-sm text-mist">Covenant tx or offer id</span>
          <input
            type="text"
            spellCheck={false}
            autoComplete="off"
            placeholder="64 hex characters"
            value={laneId}
            onChange={(e) => setLaneId(e.target.value)}
            className="w-full border border-[var(--agora-line)] bg-[rgba(14,16,20,0.55)] px-4 py-3 font-mono text-sm text-[var(--agora-ink)] outline-none focus:border-[var(--agora-gold)]"
          />
        </label>
        <button
          type="submit"
          disabled={laneBusy || !laneId.trim()}
          className="agora-btn agora-btn-primary disabled:opacity-50"
        >
          {laneBusy ? "Looking up…" : "Lookup lanes"}
        </button>
      </form>
      {laneError ? (
        <p className="mt-4 text-sm text-[var(--agora-danger)]">{laneError}</p>
      ) : null}
      {covenant ? (
        <p className="mt-4 font-mono text-sm">
          TLT covenant{" "}
          <span className="text-[var(--agora-cyan)]">{covenant.status}</span>
          {covenant.confirmations != null
            ? ` · ${covenant.confirmations} conf`
            : ""}
        </p>
      ) : null}
      {offer ? (
        <p className="mt-2 font-mono text-sm">
          DRC offer{" "}
          <span className="text-[var(--agora-cyan)]">
            {String(offer.status ?? "unknown")}
          </span>
          {offer.simulated_fill === false ? " · simulated_fill false" : ""}
        </p>
      ) : null}
    </div>
  );
}
