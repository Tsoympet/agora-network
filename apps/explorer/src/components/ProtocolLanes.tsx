import { FormEvent, useEffect, useState } from "react";
import type {
  LightAccountBalances,
  LightDrcOfferPage,
  LightValidator,
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
  if (record.receipt != null || record.policy != null) return true;
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
  const [accountPolicy, setAccountPolicy] = useState<unknown>(null);
  const [accountKeys, setAccountKeys] = useState<unknown>(null);
  const [accountSignerList, setAccountSignerList] = useState<unknown>(null);
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

  const [invoiceRecipient, setInvoiceRecipient] = useState("");
  const [invoiceId, setInvoiceId] = useState("");
  const [invoiceHit, setInvoiceHit] = useState<unknown>(null);
  const [invoiceError, setInvoiceError] = useState<string | null>(null);
  const [invoiceBusy, setInvoiceBusy] = useState(false);

  const [preauthOwner, setPreauthOwner] = useState("");
  const [preauthSource, setPreauthSource] = useState("");
  const [preauthHit, setPreauthHit] = useState<unknown>(null);
  const [preauthError, setPreauthError] = useState<string | null>(null);
  const [preauthBusy, setPreauthBusy] = useState(false);

  const [claimChannel, setClaimChannel] = useState("");
  const [claimAmount, setClaimAmount] = useState("");
  const [claimSignature, setClaimSignature] = useState("");
  const [claimHit, setClaimHit] = useState<unknown>(null);
  const [claimError, setClaimError] = useState<string | null>(null);
  const [claimBusy, setClaimBusy] = useState(false);

  const [validatorAsset, setValidatorAsset] = useState<"OVL" | "DRC">("OVL");
  const [validatorOperator, setValidatorOperator] = useState("");
  const [validatorHit, setValidatorHit] = useState<LightValidator | null>(null);
  const [validatorError, setValidatorError] = useState<string | null>(null);
  const [validatorBusy, setValidatorBusy] = useState(false);

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
    setAccountPolicy(null);
    setAccountKeys(null);
    setAccountSignerList(null);
    try {
      const next = await client.getAccountBalances(account.trim());
      setBalances(next);
      const owner = account.trim();
      const [offers, objects, policy, keys, signers] = await Promise.all([
        client.getDrcAccountOffers({ account: owner, limit: 16 }),
        client.getDrcAccountObjects({ account: owner }),
        client.getDrcAccountPolicy(owner),
        client.getDrcAccountKeys(owner),
        client.getDrcAccountSignerList(owner),
      ]);
      setAccountOffers(offers);
      setAccountObjects(objects);
      setAccountPolicy(policy);
      setAccountKeys(keys);
      setAccountSignerList(signers);
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
      ["DRC payment", client.getDrcPayment(raw)],
      ["DRC escrow", client.getDrcEscrow(raw)],
      ["DRC escrow receipt", client.getDrcEscrowReceipt(raw)],
      ["DRC check", client.getDrcCheck(raw)],
      ["DRC check receipt", client.getDrcCheckReceipt(raw)],
      ["DRC payment channel", client.getDrcPaymentChannel(raw)],
      ["DRC channel receipt", client.getDrcPaymentChannelReceipt(raw)],
      ["DRC channel fund event", client.getDrcPaymentChannelFundEvent(raw)],
      ["DRC channel claim event", client.getDrcPaymentChannelClaimEvent(raw)],
      [
        "DRC channel schedule event",
        client.getDrcPaymentChannelScheduleEvent(raw),
      ],
      [
        "DRC issued transfer receipt",
        client.getDrcIssuedTransferReceipt(raw),
      ],
      [
        "DRC issued-asset policy receipt",
        client.getDrcIssuedAssetPolicyReceipt(raw),
      ],
      [
        "DRC issuer-control receipt",
        client.getDrcTrustLineIssuerControlReceipt(raw),
      ],
      ["DRC clawback receipt", client.getDrcIssuedClawbackReceipt(raw)],
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

  async function onInvoice(e: FormEvent) {
    e.preventDefault();
    const recipient = invoiceRecipient.trim();
    const id = invoiceId.trim();
    if (!recipient || !HEX64.test(id)) {
      setInvoiceError("Recipient plus a 64-hex invoice id are required.");
      setInvoiceHit(null);
      return;
    }
    setInvoiceBusy(true);
    setInvoiceError(null);
    setInvoiceHit(null);
    try {
      setInvoiceHit(
        await client.getDrcPaymentByInvoice({
          recipient,
          invoice_id: id,
        }),
      );
    } catch (err) {
      setInvoiceError(err instanceof Error ? err.message : "invoice lookup failed");
    } finally {
      setInvoiceBusy(false);
    }
  }

  async function onPreauth(e: FormEvent) {
    e.preventDefault();
    const owner = preauthOwner.trim();
    const source = preauthSource.trim();
    if (!owner || !source) {
      setPreauthError("Owner and authorized source are required.");
      setPreauthHit(null);
      return;
    }
    setPreauthBusy(true);
    setPreauthError(null);
    setPreauthHit(null);
    try {
      setPreauthHit(
        await client.getDrcDepositPreauth({
          owner,
          authorized_source: source,
        }),
      );
    } catch (err) {
      setPreauthError(err instanceof Error ? err.message : "preauth lookup failed");
    } finally {
      setPreauthBusy(false);
    }
  }

  async function onVerifyClaim(e: FormEvent) {
    e.preventDefault();
    const channel = claimChannel.trim();
    const amount = Number(claimAmount.trim());
    const signature = claimSignature.trim();
    if (!HEX64.test(channel) || !Number.isFinite(amount) || !signature) {
      setClaimError("Channel id, amount, and claim-signature hex are required.");
      setClaimHit(null);
      return;
    }
    setClaimBusy(true);
    setClaimError(null);
    setClaimHit(null);
    try {
      setClaimHit(
        await client.verifyDrcPaymentChannelClaim({
          channel_id: channel,
          cumulative_authorized: amount,
          channel_claim_signature: signature,
        }),
      );
    } catch (err) {
      setClaimError(err instanceof Error ? err.message : "claim verify failed");
    } finally {
      setClaimBusy(false);
    }
  }

  async function onValidator(e: FormEvent) {
    e.preventDefault();
    const operator = validatorOperator.trim();
    if (!operator) {
      setValidatorError("Operator Bech32m or 40-hex is required.");
      setValidatorHit(null);
      return;
    }
    setValidatorBusy(true);
    setValidatorError(null);
    setValidatorHit(null);
    try {
      setValidatorHit(await client.getValidator(validatorAsset, operator));
    } catch (err) {
      setValidatorError(err instanceof Error ? err.message : "validator lookup failed");
    } finally {
      setValidatorBusy(false);
    }
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
            Experimental queries for native balances, DRC account policy/keys,
            every wired typed-lane id (including payments, channels, and
            issued-control receipts), invoice lookup, and off-ledger claim
            verify. Compact gossip still uses the full body for every non-UTXO
            lane. Explorer never signs.
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
              <div>
                <dt className="text-mist">account policy</dt>
                <dd>{lookupStatus(accountPolicy) ?? "—"}</dd>
              </div>
              <div>
                <dt className="text-mist">account keys</dt>
                <dd>{lookupStatus(accountKeys) ?? "—"}</dd>
              </div>
              <div>
                <dt className="text-mist">signer list</dt>
                <dd>{lookupStatus(accountSignerList) ?? "—"}</dd>
              </div>
            </dl>
          ) : null}
        </form>
      </div>

      <form onSubmit={onLane} className="mt-10 flex flex-col gap-4 md:flex-row md:items-end">
        <label className="flex min-w-0 flex-1 flex-col gap-2">
          <span className="text-sm text-mist">
            Typed-lane id (payment, channel, receipts, covenant, offer,
            escrow, check, DA, passport, hub, grant, mission, treasury,
            vesting)
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

      <div className="mt-12 grid gap-8 lg:grid-cols-2">
        <form onSubmit={onInvoice} className="flex min-w-0 flex-col gap-3">
          <p className="agora-eyebrow">DRC payment by invoice</p>
          <label className="flex min-w-0 flex-col gap-2">
            <span className="text-sm text-mist">Recipient</span>
            <input
              type="text"
              spellCheck={false}
              autoComplete="off"
              placeholder="Bech32m or 40-hex"
              value={invoiceRecipient}
              onChange={(e) => setInvoiceRecipient(e.target.value)}
              className="w-full border border-[var(--agora-line)] bg-[rgba(14,16,20,0.55)] px-4 py-3 font-mono text-sm text-[var(--agora-ink)] outline-none focus:border-[var(--agora-gold)]"
            />
          </label>
          <label className="flex min-w-0 flex-col gap-2">
            <span className="text-sm text-mist">Invoice id</span>
            <input
              type="text"
              spellCheck={false}
              autoComplete="off"
              placeholder="64 hex characters"
              value={invoiceId}
              onChange={(e) => setInvoiceId(e.target.value)}
              className="w-full border border-[var(--agora-line)] bg-[rgba(14,16,20,0.55)] px-4 py-3 font-mono text-sm text-[var(--agora-ink)] outline-none focus:border-[var(--agora-gold)]"
            />
          </label>
          <button
            type="submit"
            disabled={invoiceBusy || !invoiceRecipient.trim() || !invoiceId.trim()}
            className="agora-btn agora-btn-primary self-start disabled:opacity-50"
          >
            {invoiceBusy ? "Looking up…" : "agora_getDrcPaymentByInvoice"}
          </button>
          {invoiceError ? (
            <p className="text-sm text-[var(--agora-danger)]">{invoiceError}</p>
          ) : null}
          {invoiceHit ? (
            <p className="font-mono text-sm">
              invoice{" "}
              <span className="text-[var(--agora-cyan)]">
                {(invoiceHit as { status?: string }).status ?? "result"}
              </span>
            </p>
          ) : null}
        </form>

        <form onSubmit={onPreauth} className="flex min-w-0 flex-col gap-3">
          <p className="agora-eyebrow">DRC deposit preauth</p>
          <label className="flex min-w-0 flex-col gap-2">
            <span className="text-sm text-mist">Owner</span>
            <input
              type="text"
              spellCheck={false}
              autoComplete="off"
              placeholder="Bech32m or 40-hex"
              value={preauthOwner}
              onChange={(e) => setPreauthOwner(e.target.value)}
              className="w-full border border-[var(--agora-line)] bg-[rgba(14,16,20,0.55)] px-4 py-3 font-mono text-sm text-[var(--agora-ink)] outline-none focus:border-[var(--agora-gold)]"
            />
          </label>
          <label className="flex min-w-0 flex-col gap-2">
            <span className="text-sm text-mist">Authorized source</span>
            <input
              type="text"
              spellCheck={false}
              autoComplete="off"
              placeholder="Bech32m or 40-hex"
              value={preauthSource}
              onChange={(e) => setPreauthSource(e.target.value)}
              className="w-full border border-[var(--agora-line)] bg-[rgba(14,16,20,0.55)] px-4 py-3 font-mono text-sm text-[var(--agora-ink)] outline-none focus:border-[var(--agora-gold)]"
            />
          </label>
          <button
            type="submit"
            disabled={preauthBusy || !preauthOwner.trim() || !preauthSource.trim()}
            className="agora-btn agora-btn-primary self-start disabled:opacity-50"
          >
            {preauthBusy ? "Looking up…" : "agora_getDrcDepositPreauth"}
          </button>
          {preauthError ? (
            <p className="text-sm text-[var(--agora-danger)]">{preauthError}</p>
          ) : null}
          {preauthHit ? (
            <p className="font-mono text-sm">
              preauth{" "}
              <span className="text-[var(--agora-cyan)]">
                {(preauthHit as { status?: string }).status ?? "result"}
              </span>
            </p>
          ) : null}
        </form>
      </div>

      <form onSubmit={onVerifyClaim} className="mt-12 flex flex-col gap-3">
        <p className="agora-eyebrow">Verify channel claim</p>
        <p className="text-sm text-mist">
          Pure verification of a supplied cumulative claim signature against a
          live channel. No private keys leave the device; explorer never signs.
        </p>
        <div className="grid gap-4 md:grid-cols-3">
          <label className="flex min-w-0 flex-col gap-2">
            <span className="text-sm text-mist">Channel id</span>
            <input
              type="text"
              spellCheck={false}
              autoComplete="off"
              placeholder="64 hex characters"
              value={claimChannel}
              onChange={(e) => setClaimChannel(e.target.value)}
              className="w-full border border-[var(--agora-line)] bg-[rgba(14,16,20,0.55)] px-4 py-3 font-mono text-sm text-[var(--agora-ink)] outline-none focus:border-[var(--agora-gold)]"
            />
          </label>
          <label className="flex min-w-0 flex-col gap-2">
            <span className="text-sm text-mist">Cumulative authorized</span>
            <input
              type="text"
              spellCheck={false}
              autoComplete="off"
              placeholder="amount"
              value={claimAmount}
              onChange={(e) => setClaimAmount(e.target.value)}
              className="w-full border border-[var(--agora-line)] bg-[rgba(14,16,20,0.55)] px-4 py-3 font-mono text-sm text-[var(--agora-ink)] outline-none focus:border-[var(--agora-gold)]"
            />
          </label>
          <label className="flex min-w-0 flex-col gap-2">
            <span className="text-sm text-mist">Claim signature hex</span>
            <input
              type="text"
              spellCheck={false}
              autoComplete="off"
              placeholder="secp256k1 signature hex"
              value={claimSignature}
              onChange={(e) => setClaimSignature(e.target.value)}
              className="w-full border border-[var(--agora-line)] bg-[rgba(14,16,20,0.55)] px-4 py-3 font-mono text-sm text-[var(--agora-ink)] outline-none focus:border-[var(--agora-gold)]"
            />
          </label>
        </div>
        <button
          type="submit"
          disabled={
            claimBusy ||
            !claimChannel.trim() ||
            !claimAmount.trim() ||
            !claimSignature.trim()
          }
          className="agora-btn agora-btn-primary self-start disabled:opacity-50"
        >
          {claimBusy ? "Verifying…" : "agora_verifyDrcPaymentChannelClaim"}
        </button>
        {claimError ? (
          <p className="text-sm text-[var(--agora-danger)]">{claimError}</p>
        ) : null}
        {claimHit ? (
          <p className="font-mono text-sm">
            claim{" "}
            <span className="text-[var(--agora-cyan)]">
              {typeof claimHit === "object" &&
              claimHit !== null &&
              (claimHit as { valid?: unknown }).valid === true
                ? "valid"
                : lookupStatus(claimHit) ?? "result"}
            </span>
          </p>
        ) : null}
      </form>

      <form onSubmit={onValidator} className="mt-12 flex flex-col gap-4">
        <p className="agora-eyebrow">Validator lookup</p>
        <p className="text-sm text-mist">
          Read-only OVL / DRC operator record. Bonding stays in wallets —
          keys never leave the device.
        </p>
        <div className="grid gap-4 md:grid-cols-2">
          <label className="flex min-w-0 flex-col gap-2">
            <span className="text-sm text-mist">Stake asset</span>
            <select
              value={validatorAsset}
              onChange={(e) =>
                setValidatorAsset(e.target.value as "OVL" | "DRC")
              }
              className="w-full border border-[var(--agora-line)] bg-[rgba(14,16,20,0.55)] px-4 py-3 font-mono text-sm text-[var(--agora-ink)] outline-none focus:border-[var(--agora-gold)]"
            >
              <option value="OVL">OVL</option>
              <option value="DRC">DRC</option>
            </select>
          </label>
          <label className="flex min-w-0 flex-col gap-2">
            <span className="text-sm text-mist">Operator</span>
            <input
              type="text"
              spellCheck={false}
              autoComplete="off"
              placeholder="Bech32m or 40-hex"
              value={validatorOperator}
              onChange={(e) => setValidatorOperator(e.target.value)}
              className="w-full border border-[var(--agora-line)] bg-[rgba(14,16,20,0.55)] px-4 py-3 font-mono text-sm text-[var(--agora-ink)] outline-none focus:border-[var(--agora-gold)]"
            />
          </label>
        </div>
        <button
          type="submit"
          disabled={validatorBusy || !validatorOperator.trim()}
          className="agora-btn agora-btn-primary self-start disabled:opacity-50"
        >
          {validatorBusy ? "Looking up…" : "agora_getValidator"}
        </button>
        {validatorError ? (
          <p className="text-sm text-[var(--agora-danger)]">{validatorError}</p>
        ) : null}
        {validatorHit ? (
          <dl className="grid grid-cols-2 gap-3 font-mono text-sm">
            <div>
              <dt className="text-mist">status</dt>
              <dd className="text-[var(--agora-cyan)]">{validatorHit.status}</dd>
            </div>
            <div>
              <dt className="text-mist">commission</dt>
              <dd>{validatorHit.commission_bps} bps</dd>
            </div>
            <div>
              <dt className="text-mist">self bond</dt>
              <dd>{formatAmount(validatorHit.self_bond)}</dd>
            </div>
            <div>
              <dt className="text-mist">delegated</dt>
              <dd>{formatAmount(validatorHit.delegated)}</dd>
            </div>
          </dl>
        ) : null}
      </form>

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
