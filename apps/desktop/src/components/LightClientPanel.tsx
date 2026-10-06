import { FormEvent, useEffect, useState, type CSSProperties } from "react";
import {
  compareHeaderSpines,
  shortHash,
  verifyDrcObjectHeaderProof,
  verifyIncomingTlt,
  verifyReportedFinality,
  verifySelectedParentSpine,
  type LightClient,
  type LightHeaderRecord,
} from "../../../shared/light-client";

const fieldStyle: CSSProperties = {
  width: "100%",
  padding: "0.7rem 0.8rem",
  borderRadius: 8,
  border: "1px solid rgba(197, 152, 53, 0.35)",
  background: "rgba(16, 18, 24, 0.65)",
  color: "var(--agora-ink)",
  fontFamily: "ui-monospace, monospace",
  fontSize: "0.85rem",
};

const btnStyle: CSSProperties = {
  padding: "0.65rem 0.9rem",
  borderRadius: 8,
  border: "1px solid rgba(6, 187, 223, 0.45)",
  background: "transparent",
  color: "var(--agora-cyan)",
  fontFamily: "var(--font-ui)",
  cursor: "pointer",
};

export function LightClientPanel({
  client,
  network,
  genesisHash,
}: {
  client: LightClient;
  network: string | null;
  genesisHash: string | null;
}) {
  const [spine, setSpine] = useState<LightHeaderRecord[]>([]);
  const [spineNote, setSpineNote] = useState("Waiting for a canonical header spine.");
  const [finalityNote, setFinalityNote] = useState("Finality not checked.");
  const [txId, setTxId] = useState("");
  const [txNote, setTxNote] = useState<string | null>(null);
  const [objectId, setObjectId] = useState("");
  const [objectNote, setObjectNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!genesisHash) return;
    let cancelled = false;
    let previous: LightHeaderRecord[] = [];
    async function poll() {
      try {
        const chain = await client.getLightHeaders({ limit: 64 });
        if (cancelled) return;
        if (network && chain.network && chain.network !== network) {
          setSpineNote(`Wrong network: node spine says ${chain.network}.`);
          setSpine([]);
          return;
        }
        if (!chain.reaches_genesis) {
          throw new Error("header window does not reach genesis");
        }
        verifySelectedParentSpine(chain.headers, genesisHash!);
        const relation = compareHeaderSpines(previous, chain.headers);
        previous = chain.headers;
        setSpine(chain.headers);
        setSpineNote(
          relation === "reorg"
            ? `Reorg detected. Verified ${chain.headers.length} selected-parent header${chain.headers.length === 1 ? "" : "s"} to genesis ${shortHash(genesisHash!)}.`
            : `Verified ${chain.headers.length} selected-parent header${chain.headers.length === 1 ? "" : "s"} to genesis ${shortHash(genesisHash!)}. RandomX stays on the full node.`,
        );
        try {
          const state = verifyReportedFinality(chain.finality, chain.tip);
          const nodeState = chain.finality.node_state;
          if (nodeState && nodeState !== state) {
            throw new Error(`node finality ${nodeState} does not match stake totals`);
          }
          setFinalityNote(`${state}. OVL and DRC quorums are stake totals, not signature checks.`);
        } catch (err) {
          setFinalityNote(err instanceof Error ? err.message : "finality unverified");
        }
      } catch (err) {
        if (cancelled) return;
        setSpine([]);
        setSpineNote(err instanceof Error ? err.message : "header sync failed");
        setFinalityNote("Finality not checked.");
      }
    }
    void poll();
    const id = window.setInterval(() => void poll(), 8000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [client, genesisHash, network]);

  async function onVerifyTx(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setTxNote(null);
    try {
      const response = await client.getTltInclusionProof(txId.trim());
      const verified = verifyIncomingTlt(response, spine);
      setTxNote(`Included in ${shortHash(verified.headerHash)}. Merkle proof checked locally.`);
    } catch (err) {
      setTxNote(err instanceof Error ? err.message : "verification failed");
    } finally {
      setBusy(false);
    }
  }

  async function onQueryObject(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setObjectNote(null);
    try {
      const result = await client.getDrcObject(objectId.trim());
      try {
        verifyDrcObjectHeaderProof(result.object);
      } catch (err) {
        const reason = err instanceof Error ? err.message : "not header-proven";
        if (result.status === "unknown" || !result.object) {
          setObjectNote(`Unknown object. ${reason}`);
        } else {
          const kind =
            result.object && typeof result.object === "object" && "kind" in result.object
              ? String(result.object.kind)
              : "object";
          setObjectNote(`Node-reported ${kind}. ${reason}`);
        }
      }
    } catch (err) {
      setObjectNote(err instanceof Error ? err.message : "object query failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section style={{ marginTop: "2.75rem", maxWidth: 520 }}>
      <p className="agora-eyebrow">Light client</p>
      <p style={{ marginTop: "0.55rem", fontSize: "0.85rem", color: "var(--agora-ink-muted)" }}>
        Checks Trident header links, TLT inclusion, and separate OVL/DRC quorum totals.
        Keys stay on this device.
      </p>
      <p style={{ marginTop: "0.75rem", fontSize: "0.95rem" }}>{spineNote}</p>
      <p style={{ marginTop: "0.35rem", fontSize: "0.9rem", color: "var(--agora-cyan)" }}>
        {finalityNote}
      </p>
      <form onSubmit={onVerifyTx} style={{ marginTop: "1rem", display: "grid", gap: "0.65rem" }}>
        <input
          value={txId}
          onChange={(event) => setTxId(event.target.value)}
          placeholder="TLT tx id (64 hex)"
          aria-label="TLT transaction id"
          spellCheck={false}
          style={fieldStyle}
        />
        <button type="submit" disabled={busy || !txId.trim()} style={btnStyle}>
          Verify TLT inclusion
        </button>
      </form>
      {txNote ? (
        <p style={{ marginTop: "0.55rem", fontSize: "0.85rem" }}>{txNote}</p>
      ) : null}
      <form onSubmit={onQueryObject} style={{ marginTop: "1rem", display: "grid", gap: "0.65rem" }}>
        <input
          value={objectId}
          onChange={(event) => setObjectId(event.target.value)}
          placeholder="DRC object id (64 hex)"
          aria-label="DRC object id"
          spellCheck={false}
          style={fieldStyle}
        />
        <button type="submit" disabled={busy || !objectId.trim()} style={btnStyle}>
          Query DRC object
        </button>
      </form>
      {objectNote ? (
        <p style={{ marginTop: "0.55rem", fontSize: "0.85rem" }}>{objectNote}</p>
      ) : null}
    </section>
  );
}
