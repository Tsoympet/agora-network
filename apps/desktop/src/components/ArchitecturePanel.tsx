import { useState } from "react";
import { generateMnemonic } from "../../../shared/wallet";
import {
  exportDeviceSeed,
  lockDeviceSpendSession,
  openDeviceSpendSession,
} from "../../../shared/security/session";
import { SESSION_TIMEOUT_CHOICES } from "../../../shared/security/desktop";
import {
  architecturePanelModel,
  parseUxMode,
  type UxMode,
} from "../../../shared/settings";

const btnStyle = {
  padding: "0.55rem 0.9rem",
  border: "1px solid var(--agora-gold)",
  background: "transparent",
  color: "var(--agora-gold)",
  cursor: "pointer" as const,
  fontFamily: "var(--agora-display)",
  letterSpacing: "0.04em",
};

export function ArchitecturePanel(props: {
  nodeUrl: string;
  unlocked: boolean;
  onLock: () => void;
  timeoutMs: number;
  onTimeoutMs: (ms: number) => void;
}) {
  const [mode, setMode] = useState<UxMode>("beginner");
  const [note, setNote] = useState<string | null>(null);
  const model = architecturePanelModel(parseUxMode(mode), props.nodeUrl);

  function onRefuseExport() {
    const phrase = generateMnemonic();
    const session = openDeviceSpendSession(phrase, Date.now(), props.timeoutMs);
    lockDeviceSpendSession(session);
    try {
      exportDeviceSeed(session, Date.now(), true);
      setNote("unexpected export");
    } catch (err) {
      setNote(err instanceof Error ? err.message : "locked");
    }
  }

  return (
    <section style={{ marginTop: "1.5rem", maxWidth: 640 }}>
      <p className="agora-eyebrow">Settings</p>
      <h2 className="agora-brand" style={{ fontSize: "1.6rem" }}>
        Participation
      </h2>
      <p className="agora-meta">
        Keys stay on this device. This screen does not show a seed.
      </p>
      <div style={{ display: "flex", gap: "0.5rem", marginTop: "0.75rem" }}>
        {(["beginner", "advanced"] as const).map((item) => (
          <button
            key={item}
            type="button"
            style={btnStyle}
            onClick={() => setMode(item)}
            aria-pressed={mode === item}
          >
            {item === "beginner" ? "Beginner" : "Advanced"}
          </button>
        ))}
      </div>
      <ul style={{ marginTop: "1rem", paddingLeft: "1.1rem" }}>
        <li>{model.copy.drc}</li>
        <li>{model.copy.ovl}</li>
        <li>{model.copy.tlt}</li>
        <li>{model.copy.community}</li>
      </ul>
      <ul style={{ paddingLeft: "1.1rem" }}>
        {model.surfaces.map((item) => (
          <li key={item.id}>
            {item.label}
            {item.status === "PLANNED" ? " · PLANNED" : ""}
          </li>
        ))}
      </ul>
      {model.warnings.length > 0 ? (
        <ul style={{ paddingLeft: "1.1rem", color: "var(--agora-gold)" }}>
          {model.warnings.map((warning) => (
            <li key={warning}>{warning}</li>
          ))}
        </ul>
      ) : null}
      <p className="agora-meta">
        Hardware wallet: {model.hardwareWallet}. Stores: {model.stores.join(", ")}.
      </p>
      <label className="agora-meta" style={{ display: "block", marginTop: "0.75rem" }}>
        Lock after
        <select
          value={props.timeoutMs}
          onChange={(event) => props.onTimeoutMs(Number(event.target.value))}
          style={{ marginLeft: "0.5rem" }}
          aria-label="Session timeout"
        >
          {SESSION_TIMEOUT_CHOICES.map((ms) => (
            <option key={ms} value={ms}>
              {Math.round(ms / 60_000)} min
            </option>
          ))}
        </select>
      </label>
      <div style={{ display: "flex", gap: "0.5rem", marginTop: "0.75rem" }}>
        <button type="button" style={btnStyle} onClick={props.onLock} disabled={!props.unlocked}>
          Lock wallet
        </button>
        <button type="button" style={btnStyle} onClick={onRefuseExport}>
          Try seed export
        </button>
      </div>
      {note ? <p className="agora-meta">{note}</p> : null}
    </section>
  );
}
