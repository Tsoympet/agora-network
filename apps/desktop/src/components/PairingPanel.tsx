import { FormEvent, useEffect, useMemo, useState, type CSSProperties } from "react";
import {
  PAIRING_GUIDE_STEPS,
  buildRestorePairing,
  buildWatchPairing,
  pairingImportBlocker,
  parsePairingPayload,
  qrMatrix,
  rpcTrustWarning,
  serializePairing,
  shortHash,
  watchWithRpc,
  type ParsedPairing,
  type WatchOnlyWallet,
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

function QrMark({ text, label }: { text: string; label: string }) {
  const matrix = useMemo(() => qrMatrix(text), [text]);
  const cell = matrix.size > 80 ? 3 : 4;
  return (
    <div
      role="img"
      aria-label={label}
      style={{
        display: "grid",
        gridTemplateColumns: `repeat(${matrix.size}, ${cell}px)`,
        width: matrix.size * cell,
        background: "#f4f1ea",
        padding: 8,
        marginTop: "0.75rem",
      }}
    >
      {matrix.dark.map((on, index) => (
        <span
          key={index}
          style={{ width: cell, height: cell, background: on ? "#101218" : "#f4f1ea" }}
        />
      ))}
    </div>
  );
}

export function PairingPanel({
  network,
  genesisHash,
  rpcUrl,
  rpcToken,
  onSaveRpc,
  spendMnemonic,
  watchWallet,
  onImportWatch,
  onImportRestore,
  copyText,
}: {
  network: string | null;
  genesisHash: string | null;
  rpcUrl: string;
  rpcToken: string;
  onSaveRpc: (url: string, token: string) => Promise<void>;
  spendMnemonic: string;
  watchWallet: WatchOnlyWallet | null;
  onImportWatch: (watch: WatchOnlyWallet) => void;
  onImportRestore: (mnemonic: string) => void;
  copyText: (text: string) => Promise<boolean>;
}) {
  const [rpcDraft, setRpcDraft] = useState(rpcUrl);
  const [tokenDraft, setTokenDraft] = useState(rpcToken);
  const [rpcMsg, setRpcMsg] = useState<string | null>(null);
  const [revealArmed, setRevealArmed] = useState(false);
  const [restoreText, setRestoreText] = useState<string | null>(null);
  const [restoreError, setRestoreError] = useState<string | null>(null);
  const [importDraft, setImportDraft] = useState("");
  const [parsed, setParsed] = useState<ParsedPairing | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [importMsg, setImportMsg] = useState<string | null>(null);
  const [copyHint, setCopyHint] = useState<string | null>(null);

  useEffect(() => setRpcDraft(rpcUrl), [rpcUrl]);
  useEffect(() => setTokenDraft(rpcToken), [rpcToken]);
  useEffect(() => {
    setRestoreText(null);
    setRevealArmed(false);
  }, [spendMnemonic]);

  const watchExport = useMemo(() => {
    if (!network || !genesisHash) return null;
    try {
      const watch = spendMnemonic.trim()
        ? buildWatchPairing({
            mnemonic: spendMnemonic,
            network,
            genesis: genesisHash,
            rpcUrl,
          })
        : watchWallet
          ? watchWithRpc(watchWallet, rpcUrl)
          : null;
      if (!watch) return null;
      return {
        text: serializePairing(watch),
        omittedLoopback: watch.rpcUrl === null,
      };
    } catch {
      return null;
    }
  }, [genesisHash, network, rpcUrl, spendMnemonic, watchWallet]);

  let draftWarning: string | null = null;
  let draftError: string | null = null;
  try {
    draftWarning = rpcTrustWarning(rpcDraft);
  } catch (err) {
    draftError = err instanceof Error ? err.message : "RPC URL must be http or https";
  }

  const blocker = parsed
    ? pairingImportBlocker(
        parsed.kind === "watch" ? parsed.watch : parsed.restore,
        { network, genesis: genesisHash },
      )
    : null;

  async function onSave(event: FormEvent) {
    event.preventDefault();
    setRpcMsg(null);
    try {
      await onSaveRpc(rpcDraft, tokenDraft);
      setRpcMsg("RPC endpoint saved on this device.");
    } catch (err) {
      setRpcMsg(err instanceof Error ? err.message : "Could not save RPC URL");
    }
  }

  function onReveal() {
    if (!revealArmed || !spendMnemonic.trim() || !network || !genesisHash) return;
    try {
      setRestoreText(
        serializePairing(
          buildRestorePairing({
            mnemonic: spendMnemonic,
            network,
            genesis: genesisHash,
          }),
        ),
      );
      setRestoreError(null);
    } catch (err) {
      setRestoreText(null);
      setRestoreError(err instanceof Error ? err.message : "Could not build restore code");
    }
  }

  function stage(raw: string) {
    if (!raw.trim()) {
      setParsed(null);
      setImportDraft("");
      setImportError(null);
      return;
    }
    try {
      const next = parsePairingPayload(raw);
      setParsed(next);
      setImportDraft("");
      setImportError(null);
      setImportMsg(null);
    } catch (err) {
      setParsed(null);
      setImportDraft(raw);
      setImportError(err instanceof Error ? err.message : "malformed pairing payload");
    }
  }

  function onImport() {
    if (!parsed || blocker) return;
    if (parsed.kind === "watch") {
      onImportWatch(parsed.watch);
      setImportMsg("Watch-only imported. This device can check balances and cannot spend.");
    } else {
      onImportRestore(parsed.restore.mnemonic);
      setImportMsg(
        "Mnemonic restored in memory. Anyone with these words can spend. Save the vault, then hide the words.",
      );
    }
    setParsed(null);
  }

  return (
    <section style={{ marginTop: "2.75rem", maxWidth: 560 }}>
      <p className="agora-eyebrow">Pair with another device</p>
      <p style={{ marginTop: "0.55rem", fontSize: "0.85rem", color: "var(--agora-ink-muted)" }}>
        Device to device. Keys stay on the devices. There is no Agora login.
      </p>
      <ol style={{ marginTop: "0.75rem", paddingLeft: "1.2rem", display: "grid", gap: "0.45rem" }}>
        {PAIRING_GUIDE_STEPS.map((step) => (
          <li key={step} style={{ fontSize: "0.85rem", color: "var(--agora-ink-muted)" }}>
            {step}
          </li>
        ))}
      </ol>
      <p style={{ marginTop: "0.85rem", fontSize: "0.85rem" }}>
        This node: {network ?? "not connected"}
        {genesisHash ? ` · genesis ${shortHash(genesisHash)}` : " · genesis not reported yet"}
      </p>

      <form onSubmit={onSave} style={{ marginTop: "1rem", display: "grid", gap: "0.65rem" }}>
        <label style={{ fontSize: "0.85rem" }}>
          RPC endpoint (http or https)
          <input
            value={rpcDraft}
            onChange={(event) => setRpcDraft(event.target.value)}
            placeholder="https://your-node.example:8545/rpc"
            aria-label="RPC endpoint"
            spellCheck={false}
            style={{ ...fieldStyle, marginTop: "0.35rem" }}
          />
        </label>
        <label style={{ fontSize: "0.85rem" }}>
          Node access token (optional, stays on this device)
          <input
            type="password"
            value={tokenDraft}
            onChange={(event) => setTokenDraft(event.target.value)}
            placeholder="only if the node requires AGORA_RPC_TOKEN"
            aria-label="Node access token"
            autoComplete="off"
            style={{ ...fieldStyle, marginTop: "0.35rem" }}
          />
        </label>
        <button type="submit" style={btnStyle}>
          Save RPC endpoint
        </button>
      </form>
      {draftError ? (
        <p style={{ marginTop: "0.55rem", color: "var(--agora-danger)", fontSize: "0.85rem" }}>
          {draftError}
        </p>
      ) : draftWarning ? (
        <p style={{ marginTop: "0.55rem", fontSize: "0.85rem", color: "var(--agora-gold)" }}>
          {draftWarning}
        </p>
      ) : null}
      {rpcMsg ? (
        <p style={{ marginTop: "0.35rem", fontSize: "0.85rem" }}>{rpcMsg}</p>
      ) : null}

      <h2 style={{ marginTop: "1.5rem", fontSize: "1rem", fontFamily: "var(--agora-display)", color: "var(--agora-gold)" }}>
        Watch-only (recommended for travel)
      </h2>
      <p style={{ marginTop: "0.45rem", fontSize: "0.85rem", color: "var(--agora-ink-muted)" }}>
        Public TLT receive and change addresses, the OVL account id, the DRC account id, and the
        account xpub. The other device can check balances away from home and cannot spend.
      </p>
      {watchExport ? (
        <>
          <QrMark text={watchExport.text} label="Watch-only pairing code" />
          {watchExport.omittedLoopback ? (
            <p style={{ marginTop: "0.55rem", fontSize: "0.85rem" }}>
              This computer&apos;s RPC is localhost, so it was left out of the code. On the phone,
              enter a LAN or public RPC before you leave home.
            </p>
          ) : null}
          <button
            type="button"
            style={{ ...btnStyle, marginTop: "0.75rem" }}
            onClick={() => {
              void copyText(watchExport.text).then((ok) => {
                setCopyHint(ok ? "Copied watch-only payload" : "Clipboard unavailable");
              });
            }}
          >
            Copy watch-only payload
          </button>
        </>
      ) : (
        <p style={{ marginTop: "0.55rem", fontSize: "0.85rem" }}>
          Unlock or create a wallet, and wait for this node to report genesis, to show a watch-only code.
        </p>
      )}

      <h2 style={{ marginTop: "1.5rem", fontSize: "1rem", fontFamily: "var(--agora-display)", color: "var(--agora-gold)" }}>
        Same-spend restore
      </h2>
      <p style={{ marginTop: "0.45rem", fontSize: "0.85rem", color: "var(--agora-danger)" }}>
        Anyone who sees this mnemonic can spend. The reveal-once code stays on this screen only
        until you hide it. It is not sent to a node.
      </p>
      {spendMnemonic.trim() && network && genesisHash ? (
        <>
          <label style={{ display: "flex", gap: "0.5rem", marginTop: "0.75rem", fontSize: "0.85rem" }}>
            <input
              type="checkbox"
              checked={revealArmed}
              onChange={(event) => setRevealArmed(event.target.checked)}
            />
            I understand anyone with the mnemonic can spend
          </label>
          <div style={{ display: "flex", gap: "0.75rem", marginTop: "0.75rem" }}>
            <button type="button" disabled={!revealArmed} onClick={onReveal} style={btnStyle}>
              Show restore code once
            </button>
            {restoreText ? (
              <button
                type="button"
                style={btnStyle}
                onClick={() => {
                  setRestoreText(null);
                  setRevealArmed(false);
                }}
              >
                Hide restore code
              </button>
            ) : null}
          </div>
          {restoreText ? (
            <>
              <QrMark text={restoreText} label="Reveal-once restore code" />
              <button
                type="button"
                style={{ ...btnStyle, marginTop: "0.75rem" }}
                onClick={() => {
                  void copyText(restoreText).then((ok) => {
                    setCopyHint(
                      ok
                        ? "Copied restore payload. Hide it after the other device imports."
                        : "Clipboard unavailable",
                    );
                  });
                }}
              >
                Copy restore payload
              </button>
            </>
          ) : null}
        </>
      ) : (
        <p style={{ marginTop: "0.55rem", fontSize: "0.85rem" }}>
          Unlock the spend wallet on this device to reveal a restore code. A watch-only session
          cannot create one.
        </p>
      )}
      {restoreError ? (
        <p style={{ marginTop: "0.55rem", color: "var(--agora-danger)", fontSize: "0.85rem" }}>
          {restoreError}
        </p>
      ) : null}

      <h2 style={{ marginTop: "1.5rem", fontSize: "1rem", fontFamily: "var(--agora-display)", color: "var(--agora-gold)" }}>
        Import a pairing payload
      </h2>
      <textarea
        value={importDraft}
        onChange={(event) => stage(event.target.value)}
        placeholder="Paste a watch-only or restore payload"
        aria-label="Pairing payload"
        rows={3}
        spellCheck={false}
        style={{ ...fieldStyle, marginTop: "0.75rem", resize: "vertical" }}
      />
      {importError ? (
        <p style={{ marginTop: "0.55rem", color: "var(--agora-danger)", fontSize: "0.85rem" }}>
          {importError}
        </p>
      ) : null}
      {parsed ? (
        <div style={{ marginTop: "0.75rem", fontSize: "0.85rem" }}>
          {parsed.kind === "watch" ? (
            <>
              <p>Watch-only for {parsed.watch.network}.</p>
              <p style={{ wordBreak: "break-all" }}>TLT receive {parsed.watch.tltReceive}</p>
              <p style={{ wordBreak: "break-all" }}>TLT change {parsed.watch.tltChange}</p>
              <p>OVL account {parsed.watch.ovlAccount}</p>
              <p>DRC account {parsed.watch.drcAccount}</p>
              <p>Genesis {shortHash(parsed.watch.genesis)}</p>
            </>
          ) : (
            <p>
              Same-spend restore for {parsed.restore.network}. Genesis{" "}
              {shortHash(parsed.restore.genesis)}. The mnemonic is held for import and is not shown.
            </p>
          )}
          {blocker ? (
            <p style={{ color: "var(--agora-danger)" }}>{blocker}</p>
          ) : (
            <p>Network and genesis match this node.</p>
          )}
          <div style={{ display: "flex", gap: "0.75rem", marginTop: "0.65rem" }}>
            <button type="button" disabled={!!blocker} onClick={onImport} style={btnStyle}>
              Import
            </button>
            <button
              type="button"
              style={btnStyle}
              onClick={() => {
                setParsed(null);
                setImportDraft("");
              }}
            >
              Discard
            </button>
          </div>
        </div>
      ) : null}
      {importMsg ? (
        <p style={{ marginTop: "0.55rem", fontSize: "0.85rem" }}>{importMsg}</p>
      ) : null}
      {copyHint ? (
        <p style={{ marginTop: "0.35rem", fontSize: "0.8rem", color: "var(--agora-ink-muted)" }}>
          {copyHint}
        </p>
      ) : null}
    </section>
  );
}
