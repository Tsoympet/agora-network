import { useEffect, useState } from "react";
import {
  networkAccent,
  networkHrpHint,
  networkLabel,
} from "../../../shared/light-client";
import { getNodeInfo, type LightNodeInfo } from "../lib/rpc";

const POLL_MS = Number(import.meta.env.VITE_AGORA_POLL_MS) || 4000;

/** Compact Devnet / Testnet / Mainnet indicator for chrome. */
export function NetworkBadge() {
  const [info, setInfo] = useState<LightNodeInfo | null>(null);

  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;

    const tick = async () => {
      try {
        const next = await getNodeInfo();
        if (!cancelled) setInfo(next);
      } catch {
        if (!cancelled) setInfo(null);
      } finally {
        if (!cancelled) {
          timer = window.setTimeout(tick, POLL_MS);
        }
      }
    };

    void tick();
    return () => {
      cancelled = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, []);

  const accent = networkAccent(info?.network ?? "devnet");
  const label = info ? networkLabel(info.network) : "Connecting…";
  const hrp = networkHrpHint(info?.network ?? "devnet");

  return (
    <span
      className="inline-flex items-center gap-2 border-b-2 pb-1 font-[family-name:var(--agora-display)] text-xs uppercase tracking-[0.14em] md:text-sm"
      style={{ color: accent, borderColor: accent }}
      title={
        info
          ? `Connected node reports network=${info.network}`
          : "Waiting for agora_getNodeInfo"
      }
      aria-live="polite"
    >
      <span
        className="inline-block h-2 w-2 rounded-full"
        style={{ background: accent }}
        aria-hidden
      />
      {label}
      <span className="font-mono text-[0.7rem] normal-case tracking-normal opacity-75">
        {hrp}
      </span>
    </span>
  );
}
