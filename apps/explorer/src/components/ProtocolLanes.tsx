import { FormEvent, useEffect, useState } from "react";
import type {
  LightAccountBalances,
  LightDrcOfferPage,
  RpcStatus,
} from "../../../shared/light-client";
import { getClient, rpcUrl } from "../lib/rpc";

const HEX64 = /^(0x)?[0-9a-fA-F]{64}$/;

function formatAmount(value: number | string | undefined): string {
  if (value === undefined) return "—";
  return String(value).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

function lookupStatus(value: unknown): string | null {
  if (value == null || typeof value !== "object") return null;
  const status = (value as { status?: unknown }).status;
  if (typeof status !== "string") return null;
  if (status === "unknown" || status === "not_found") return null;
  return status;
}

function hasLivePayload(value: unknown): boolean {
  if (value == null || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  if (lookupStatus(value)) return true;
  if (Array.isArray(record.offers) && record.offers.length > 0) return true;
  if (Array.isArray(record.objects) && record.objects.length > 0) return true;
  if (record.object != null || record.authorization != null) return true;
  return false;
}

type LaneHit = { label: string; status: string; detail: unknown };

export function ProtocolLanes() {
  const client = getClient();
  const [ethChain, setEthChain] = useState<string | null>(null);
  const [ethBlock, setEthBlock] = useState<string | null>(null);
  const [ethStatus, setEthStatus] = useState<RpcStatus>("idle");

  const [account, setAccount] = useState("");
  const [balances, setBalances] = useState<LightAccountBalances | null>(null);
  const [accountOffers, setAccountOffers] = useState<LightDrcOfferPage | null>(
    null,
  );
  const [accountObjects, setAccountObjects] = useState<unknown>(null);
  const [accountError, setAccountError] = useState<string | null>(null);
  const [accountBusy, setAccountBusy] = useState(false);

  const [laneId, setLaneId] = useState("");
  const [laneHits, setLaneHits] = useState<LaneHit[]>([]);
  const [laneError, setLaneError] = useState<string | null>(null);
  const [laneBusy, setLaneBusy] = useState(false);

  const [bookIssuer, setBookIssuer] = useState("");
  const [bookCurrency, setBookCurrency] = useState("USD");
  const [bookDirection, setBookDirection] = useState<"drc-pays" | "issued-pays">(
    "drc-pays",
  );
  const [bookPage, setBookPage] = useState<LightDrcOfferPage | null>(null);
  const [bookError, setBookError] = useState<string | null>(null);
  const [bookBusy, setBookBusy] = useState(false);

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
    setAccountOffers(null);
    setAccountObjects(null);
    try {
      const next = await client.getAccountBalances(account.trim());
      setBalances(next);
      const [offers, objects] = await Promise.all([
        client.getDrcAccountOffers({ account: account.trim(), limit: 16 }),
        client.getDrcAccountObjects({ account: account.trim() }),
      ]);
      setAccountOffers(offers);
      setAccountObjects(objects);
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
      setLaneError("Enter a 64-hex object, attestation, or envelope id.");
      setLaneHits([]);
      return;
    }
    setLaneBusy(true);
    setLaneError(null);
    setLaneHits([]);
    const queries: Array<[string, Promise<unknown>]> = [
      ["TLT covenant", client.getTltCovenant(raw)],
      ["DRC offer", client.getDrcOffer(raw)],
      ["DRC escrow", client.getDrcEscrow(raw)],
      ["DRC check", client.getDrcCheck(raw)],
      ["DRC ledger object", client.getDrcObject(raw)],
      ["DA commitment", client.getDataCommitment({ authorization_id: raw })],
      ["Passport attestation", client.getPassportAttestation(raw)],
      ["Hub registration", client.getHubRegistration(raw)],
      ["Grant registration", client.getGrantRegistration(raw)],
      ["Mission registration", client.getMissionRegistration(raw)],
      ["Treasury disbursement", client.getTreasuryDisbursement(raw)],
      ["Vesting unlock", client.getVestingUnlock(raw)],
    ];
    const settled = await Promise.allSettled(
      queries.map(async ([label, promise]) => {
        const value = await promise;
        const status = lookupStatus(value);
        if (!status && !hasLivePayload(value)) return null;
        return { label, status: status ?? "live", detail: value } satisfies LaneHit;
      }),
    );
    const hits = settled.flatMap((result) =>
      result.status === "fulfilled" && result.value ? [result.value] : [],
    );
    setLaneHits(hits);
    if (hits.length === 0) {
      setLaneError("No live typed-lane object for that id on this node.");
    }
    setLaneBusy(false);
  }

  async function onBook(e: FormEvent) {
    e.preventDefault();
    const issuer = bookIssuer.trim();
    const currency = bookCurrency.trim();
    if (!issuer || !currency) {
      setBookError("Issuer and uppercase currency (or 40-hex) are required.");
      return;
    }
    setBookBusy(true);
    setBookError(null);
    setBookPage(null);
    const issued = { type: "issued" as const, issuer, currency };
    const native = { type: "native_drc" as const };
    const book =
      bookDirection === "drc-pays"
        ? { pays: native, gets: issued }
        : { pays: issued, gets: native };
    try {
      const page = await client.getDrcBookOffers({ book, limit: 16 });
      setBookPage(page);
    } catch (err) {
      setBookError(err instanceof Error ? err.message : "book lookup failed");
    } finally {
      setBookBusy(false);
    }
  }

  const bookOffers = Array.isArray(bookPage?.offers) ? bookPage.offers : [];
  const ownedOffers = Array.isArray(accountOffers?.offers)
    ? accountOffers.offers
    : [];

  return (
    <div className="relative">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="agora-eyebrow">Protocol lanes</p>
          <h2 className="agora-display mt-3 text-3xl md:text-4xl">
            TLT · DRC · OVL reads
          </h2>
          <p className="agora-lede mt-4">
            Experimental queries for native balances, every wired typed-lane
            id, DRC account objects, and the native DEX book. Compact gossip
            still uses the full body for every non-UTXO lane. Explorer never
            signs.
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
            {accountBusy ? "Looking up…" : "Balances + DRC objects"}
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
              <div>
                <dt className="text-mist">DRC offers</dt>
                <dd>{ownedOffers.length}</dd>
              </div>
              <div>
                <dt className="text-mist">DRC objects</dt>
                <dd>
                  {accountObjects &&
                  typeof accountObjects === "object" &&
                  Array.isArray((accountObjects as { objects?: unknown }).objects)
                    ? (accountObjects as { objects: unknown[] }).objects.length
                    : "—"}
                </dd>
              </div>
            </dl>
          ) : null}
        </form>
      </div>

      <form onSubmit={onLane} className="mt-10 flex flex-col gap-4 md:flex-row md:items-end">
        <label className="flex min-w-0 flex-1 flex-col gap-2">
          <span className="text-sm text-mist">
            Typed-lane id (covenant, offer, escrow, check, DA, passport, hub,
            grant, mission, treasury, vesting)
          </span>
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
      {laneHits.map((hit) => (
        <p key={hit.label} className="mt-3 font-mono text-sm">
          {hit.label}{" "}
          <span className="text-[var(--agora-cyan)]">{hit.status}</span>
        </p>
      ))}

      <form onSubmit={onBook} className="mt-12 flex flex-col gap-4">
        <p className="agora-eyebrow">DRC native DEX book</p>
        <p className="text-sm text-mist">
          Read-only book page. Order entry stays in wallets. Native DRC versus
          one issued asset.
        </p>
        <div className="grid gap-4 md:grid-cols-3">
          <label className="flex min-w-0 flex-col gap-2">
            <span className="text-sm text-mist">Issued issuer</span>
            <input
              type="text"
              spellCheck={false}
              autoComplete="off"
              placeholder="Bech32m or 40-hex"
              value={bookIssuer}
              onChange={(e) => setBookIssuer(e.target.value)}
              className="w-full border border-[var(--agora-line)] bg-[rgba(14,16,20,0.55)] px-4 py-3 font-mono text-sm text-[var(--agora-ink)] outline-none focus:border-[var(--agora-gold)]"
            />
          </label>
          <label className="flex min-w-0 flex-col gap-2">
            <span className="text-sm text-mist">Currency</span>
            <input
              type="text"
              spellCheck={false}
              autoComplete="off"
              placeholder="USD or 40-hex"
              value={bookCurrency}
              onChange={(e) => setBookCurrency(e.target.value)}
              className="w-full border border-[var(--agora-line)] bg-[rgba(14,16,20,0.55)] px-4 py-3 font-mono text-sm text-[var(--agora-ink)] outline-none focus:border-[var(--agora-gold)]"
            />
          </label>
          <label className="flex min-w-0 flex-col gap-2">
            <span className="text-sm text-mist">Taker direction</span>
            <select
              value={bookDirection}
              onChange={(e) =>
                setBookDirection(e.target.value as "drc-pays" | "issued-pays")
              }
              className="w-full border border-[var(--agora-line)] bg-[rgba(14,16,20,0.55)] px-4 py-3 font-mono text-sm text-[var(--agora-ink)] outline-none focus:border-[var(--agora-gold)]"
            >
              <option value="drc-pays">Pays native DRC, gets issued</option>
              <option value="issued-pays">Pays issued, gets native DRC</option>
            </select>
          </label>
        </div>
        <button
          type="submit"
          disabled={bookBusy || !bookIssuer.trim() || !bookCurrency.trim()}
          className="agora-btn agora-btn-primary self-start disabled:opacity-50"
        >
          {bookBusy ? "Looking up…" : "agora_getDrcBookOffers"}
        </button>
        {bookError ? (
          <p className="text-sm text-[var(--agora-danger)]">{bookError}</p>
        ) : null}
        {bookPage ? (
          <p className="font-mono text-sm">
            Book page{" "}
            <span className="text-[var(--agora-cyan)]">{bookOffers.length}</span>
            {" offers"}
            {bookPage.simulated_fill === false ? " · simulated_fill false" : ""}
          </p>
        ) : null}
      </form>
    </div>
  );
}
