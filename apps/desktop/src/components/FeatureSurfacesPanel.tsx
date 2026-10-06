import { FormEvent, useEffect, useMemo, useState, type CSSProperties } from "react";
import {
  LIGHT_FEATURE_MATRIX,
  matrixWithRpcAvailability,
  type FeatureCapability,
  type FeatureDomain,
} from "../../../shared/light-client/featureMatrix";
import {
  fetchTltHistory,
  submitDrcNativePayment,
  submitOvlTransfer,
} from "../../../shared/light-client/featureSurfaceActions";
import { parseU64 } from "../../../shared/light-client/borshWire";
import {
  probeLightClientRpc,
  rpcMethodsFromMatrix,
} from "../../../shared/light-client/rpcProbe";
import {
  verificationLabel,
  verificationShort,
  type VerificationStatus,
} from "../../../shared/light-client/verificationStatus";
import type { LightClient, LightUtxo } from "../../../shared/light-client";
import type { WatchOnlyWallet } from "../../../shared/light-client";

const fieldStyle: CSSProperties = {
  width: "100%",
  padding: "0.65rem 0.75rem",
  border: "1px solid color-mix(in srgb, var(--agora-gold) 35%, transparent)",
  background: "color-mix(in srgb, var(--agora-obsidian) 55%, transparent)",
  color: "var(--agora-ink)",
  fontFamily: "ui-monospace, monospace",
  fontSize: "0.85rem",
};

const btnStyle: CSSProperties = {
  padding: "0.55rem 0.9rem",
  border: "1px solid var(--agora-gold)",
  background: "transparent",
  color: "var(--agora-gold)",
  cursor: "pointer",
  fontFamily: "var(--font-ui)",
  fontSize: "0.85rem",
};

const TABS: FeatureDomain[] = ["network", "tlt", "drc", "ovl"];

function badgeColor(status: VerificationStatus): string {
  if (status === "verified-locally") return "var(--agora-cyan)";
  if (status === "node-reported") return "var(--agora-gold)";
  return "var(--agora-ink-muted)";
}

type Props = {
  client: LightClient;
  network: string | null;
  genesisHash: string | null;
  chainId: string | null;
  tltHex: string | null;
  drcHex: string | null;
  ovlHex: string | null;
  utxos: LightUtxo[];
  spendMnemonic: string;
  watchWallet: WatchOnlyWallet | null;
};

export function FeatureSurfacesPanel({
  client,
  network,
  genesisHash,
  chainId,
  tltHex,
  drcHex,
  ovlHex,
  utxos,
  spendMnemonic,
  watchWallet,
}: Props) {
  const [tab, setTab] = useState<FeatureDomain>("network");
  const [matrix, setMatrix] = useState<FeatureCapability[]>(LIGHT_FEATURE_MATRIX);
  const [feeNote, setFeeNote] = useState<string | null>(null);
  const [history, setHistory] = useState<Array<{ tx_id: string; status: string }>>([]);
  const [supplyNote, setSupplyNote] = useState<string | null>(null);
  const [validatorNote, setValidatorNote] = useState<string | null>(null);
  const [policyJson, setPolicyJson] = useState<string | null>(null);
  const [objectsJson, setObjectsJson] = useState<string | null>(null);
  const [lookupId, setLookupId] = useState("");
  const [lookupNote, setLookupNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  const [ovlTo, setOvlTo] = useState("");
  const [ovlAmt, setOvlAmt] = useState("1");
  const [ovlFee, setOvlFee] = useState("1");
  const [ovlNonce, setOvlNonce] = useState("0");

  const [drcTo, setDrcTo] = useState("");
  const [drcAmt, setDrcAmt] = useState("1");
  const [drcFee, setDrcFee] = useState("1");
  const [drcNonce, setDrcNonce] = useState("0");

  const canSign = Boolean(spendMnemonic.trim() && !watchWallet);

  const rows = useMemo(
    () => matrix.filter((row) => row.domain === tab),
    [matrix, tab],
  );

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const methods = rpcMethodsFromMatrix(LIGHT_FEATURE_MATRIX);
      const available = await probeLightClientRpc(client, methods);
      if (!cancelled) {
        setMatrix(matrixWithRpcAvailability(LIGHT_FEATURE_MATRIX, available));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client]);

  useEffect(() => {
    if (tab !== "tlt") return;
    void client
      .estimateFee()
      .then((est) => setFeeNote(`min ${est.min_relay_fee} · suggested ${est.suggested_fee}`))
      .catch(() => setFeeNote("fee RPC unavailable"));
  }, [client, tab]);

  useEffect(() => {
    if (tab !== "network") return;
    void (async () => {
      try {
        const [ovl, drc, drcSupply] = await Promise.all([
          client.getValidatorSet("OVL"),
          client.getValidatorSet("DRC"),
          client.getNativeAssetSupply("DRC"),
        ]);
        setValidatorNote(
          `OVL epoch ${ovl.epoch} · ${ovl.validators.length} validators · DRC epoch ${drc.epoch} · ${drc.validators.length} validators`,
        );
        setSupplyNote(
          `DRC net ${drcSupply.net_supply} (issued ${drcSupply.issued_supply}, burned ${drcSupply.burned_supply})`,
        );
      } catch {
        setValidatorNote("validator / supply RPC offline");
        setSupplyNote(null);
      }
    })();
  }, [client, tab]);

  async function onRefreshHistory() {
    setBusy(true);
    try {
      const rows = await fetchTltHistory(client, utxos, []);
      setHistory(rows);
    } finally {
      setBusy(false);
    }
  }

  async function onLoadDrcAccount() {
    if (!drcHex) return;
    setBusy(true);
    setPolicyJson(null);
    setObjectsJson(null);
    try {
      const [policy, objects] = await Promise.all([
        client.getDrcAccountPolicy(drcHex),
        client.getDrcAccountObjects({ account: drcHex, limit: 16 }),
      ]);
      setPolicyJson(JSON.stringify(policy, null, 2));
      setObjectsJson(JSON.stringify(objects, null, 2));
    } catch (err) {
      setPolicyJson(err instanceof Error ? err.message : "policy load failed");
    } finally {
      setBusy(false);
    }
  }

  async function onPointLookup(event: FormEvent) {
    event.preventDefault();
    const id = lookupId.trim();
    if (!id) return;
    setBusy(true);
    setLookupNote(null);
    try {
      if (id.length === 64) {
        const tries = [
          () => client.getDrcPayment(id),
          () => client.getDrcEscrow(id),
          () => client.getDrcCheck(id),
          () => client.getDrcPaymentChannel(id),
          () => client.getDrcObject(id),
        ];
        for (const run of tries) {
          try {
            const result = await run();
            setLookupNote(JSON.stringify(result, null, 2));
            return;
          } catch {
            /* try next family */
          }
        }
      }
      setLookupNote("no matching DRC point query for that id");
    } finally {
      setBusy(false);
    }
  }

  async function onOvlSend(event: FormEvent) {
    event.preventDefault();
    if (!canSign || !network || !genesisHash) {
      setMsg("Unlock a spend wallet and wait for genesis.");
      return;
    }
    setBusy(true);
    setMsg(null);
    try {
      const ref = await submitOvlTransfer({
        client,
        mnemonic: spendMnemonic,
        network,
        genesisHex: genesisHash,
        chainId: chainId ?? "",
        toHex: ovlTo.trim(),
        amount: parseU64(ovlAmt, "amount"),
        fee: parseU64(ovlFee, "fee"),
        nonce: parseU64(ovlNonce, "nonce"),
      });
      setMsg(`OVL transfer submitted (${ref})`);
    } catch (err) {
      setMsg(err instanceof Error ? err.message : "OVL send failed");
    } finally {
      setBusy(false);
    }
  }

  async function onDrcSend(event: FormEvent) {
    event.preventDefault();
    if (!canSign || !network || !genesisHash) {
      setMsg("Unlock a spend wallet and wait for genesis.");
      return;
    }
    setBusy(true);
    setMsg(null);
    try {
      const ref = await submitDrcNativePayment({
        client,
        mnemonic: spendMnemonic,
        network,
        genesisHex: genesisHash,
        chainId: chainId ?? "",
        toHex: drcTo.trim(),
        amount: parseU64(drcAmt, "amount"),
        fee: parseU64(drcFee, "fee"),
        nonce: parseU64(drcNonce, "nonce"),
      });
      setMsg(`DRC payment submitted (${ref})`);
    } catch (err) {
      setMsg(err instanceof Error ? err.message : "DRC payment failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section style={{ marginTop: "2.75rem", maxWidth: 560 }}>
      <p className="agora-eyebrow">Capabilities</p>
      <p style={{ marginTop: "0.55rem", fontSize: "0.85rem", color: "var(--agora-ink-muted)" }}>
        Almost every Agora Network read the node exposes, with honest verification labels.
        Watch-only wallets query; spend wallets sign typed envelopes locally.
      </p>
      <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap", marginTop: "1rem" }}>
        {TABS.map((key) => (
          <button
            key={key}
            type="button"
            onClick={() => setTab(key)}
            style={{
              ...btnStyle,
              borderColor: tab === key ? "var(--agora-cyan)" : btnStyle.border as string,
              color: tab === key ? "var(--agora-cyan)" : btnStyle.color,
            }}
          >
            {key.toUpperCase()}
          </button>
        ))}
      </div>
      <ul style={{ marginTop: "1rem", padding: 0, listStyle: "none", display: "grid", gap: "0.55rem" }}>
        {rows.map((row) => (
          <li
            key={row.id}
            style={{
              border: "1px solid color-mix(in srgb, var(--agora-gold) 22%, transparent)",
              borderRadius: 8,
              padding: "0.55rem 0.7rem",
              fontSize: "0.85rem",
            }}
          >
            <div style={{ display: "flex", justifyContent: "space-between", gap: "0.5rem" }}>
              <strong>{row.title}</strong>
              <span
                style={{
                  color: badgeColor(row.status),
                  fontFamily: "ui-monospace, monospace",
                  fontSize: "0.75rem",
                  whiteSpace: "nowrap",
                }}
                title={verificationLabel(row.status)}
              >
                {verificationShort(row.status)}
              </span>
            </div>
            <p style={{ margin: "0.35rem 0 0", color: "var(--agora-ink-muted)", fontSize: "0.8rem" }}>
              {row.detail}
            </p>
          </li>
        ))}
      </ul>

      {tab === "network" ? (
        <div style={{ marginTop: "1rem", fontSize: "0.85rem" }}>
          <p>
            Genesis{" "}
            <span style={{ fontFamily: "ui-monospace, monospace", color: "var(--agora-cyan)" }}>
              {genesisHash ? `${genesisHash.slice(0, 12)}…` : "waiting for node"}
            </span>
          </p>
          {validatorNote ? <p style={{ marginTop: "0.45rem" }}>{validatorNote}</p> : null}
          {supplyNote ? <p style={{ marginTop: "0.35rem", color: "var(--agora-ink-muted)" }}>{supplyNote}</p> : null}
        </div>
      ) : null}

      {tab === "tlt" ? (
        <div style={{ marginTop: "1rem", display: "grid", gap: "0.65rem" }}>
          {feeNote ? <p style={{ fontSize: "0.85rem" }}>Fee estimate · {feeNote}</p> : null}
          <button type="button" disabled={busy} onClick={() => void onRefreshHistory()} style={btnStyle}>
            Refresh UTXO-linked history
          </button>
          {history.length ? (
            <ul style={{ paddingLeft: "1.1rem", fontSize: "0.8rem", fontFamily: "ui-monospace, monospace" }}>
              {history.map((row) => (
                <li key={row.tx_id}>
                  {row.tx_id.slice(0, 10)}… · {row.status}
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}

      {tab === "drc" ? (
        <div style={{ marginTop: "1rem", display: "grid", gap: "0.75rem" }}>
          <button
            type="button"
            disabled={busy || !drcHex}
            onClick={() => void onLoadDrcAccount()}
            style={btnStyle}
          >
            Load account policy + objects
          </button>
          {policyJson ? (
            <pre style={{ fontSize: "0.72rem", overflow: "auto", maxHeight: 160 }}>{policyJson}</pre>
          ) : null}
          {objectsJson ? (
            <pre style={{ fontSize: "0.72rem", overflow: "auto", maxHeight: 160 }}>{objectsJson}</pre>
          ) : null}
          <form onSubmit={onPointLookup} style={{ display: "grid", gap: "0.55rem" }}>
            <input
              value={lookupId}
              onChange={(e) => setLookupId(e.target.value)}
              placeholder="64-hex payment / escrow / check / channel / object id"
              style={fieldStyle}
            />
            <button type="submit" disabled={busy || !lookupId.trim()} style={btnStyle}>
              Point lookup
            </button>
          </form>
          {lookupNote ? (
            <pre style={{ fontSize: "0.72rem", overflow: "auto", maxHeight: 200 }}>{lookupNote}</pre>
          ) : null}
          <form onSubmit={onDrcSend} style={{ display: "grid", gap: "0.55rem" }}>
            <input value={drcTo} onChange={(e) => setDrcTo(e.target.value)} placeholder="to address" style={fieldStyle} />
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: "0.5rem" }}>
              <input value={drcAmt} onChange={(e) => setDrcAmt(e.target.value)} placeholder="amount" style={fieldStyle} />
              <input value={drcFee} onChange={(e) => setDrcFee(e.target.value)} placeholder="fee" style={fieldStyle} />
              <input value={drcNonce} onChange={(e) => setDrcNonce(e.target.value)} placeholder="nonce" style={fieldStyle} />
            </div>
            <button type="submit" disabled={busy || !canSign} style={btnStyle}>
              Sign DRC payment
            </button>
          </form>
        </div>
      ) : null}

      {tab === "ovl" ? (
        <form onSubmit={onOvlSend} style={{ marginTop: "1rem", display: "grid", gap: "0.55rem" }}>
          <input value={ovlTo} onChange={(e) => setOvlTo(e.target.value)} placeholder="to address" style={fieldStyle} />
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: "0.5rem" }}>
            <input value={ovlAmt} onChange={(e) => setOvlAmt(e.target.value)} placeholder="amount" style={fieldStyle} />
            <input value={ovlFee} onChange={(e) => setOvlFee(e.target.value)} placeholder="fee" style={fieldStyle} />
            <input value={ovlNonce} onChange={(e) => setOvlNonce(e.target.value)} placeholder="nonce" style={fieldStyle} />
          </div>
          <button type="submit" disabled={busy || !canSign} style={btnStyle}>
            Sign OVL transfer
          </button>
          <p style={{ fontSize: "0.8rem", color: "var(--agora-ink-muted)" }}>
            OVL account {ovlHex ? `${ovlHex.slice(0, 10)}…` : "derive wallet first"}. EVM deploy stays unavailable on this base.
          </p>
        </form>
      ) : null}

      {msg ? <p style={{ marginTop: "0.75rem", fontSize: "0.85rem", color: "var(--agora-cyan)" }}>{msg}</p> : null}
      {watchWallet ? (
        <p style={{ marginTop: "0.65rem", fontSize: "0.8rem", color: "var(--agora-ink-muted)" }}>
          Watch-only: queries only. Sign &amp; send paths fail closed.
        </p>
      ) : null}
    </section>
  );
}
