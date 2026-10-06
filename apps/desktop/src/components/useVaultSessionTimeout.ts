import { useEffect, useRef } from "react";

/** Lock the in-memory mnemonic after inactivity. The callback is read from a ref so the timer is not reset every render. */
export function useVaultSessionTimeout(
  unlocked: boolean,
  onLock: () => void,
  timeoutMs: number,
): void {
  const lockRef = useRef(onLock);
  lockRef.current = onLock;
  useEffect(() => {
    if (!unlocked || timeoutMs < 1_000) return;
    let timer = window.setTimeout(() => lockRef.current(), timeoutMs);
    const bump = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => lockRef.current(), timeoutMs);
    };
    window.addEventListener("pointerdown", bump);
    window.addEventListener("keydown", bump);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("pointerdown", bump);
      window.removeEventListener("keydown", bump);
    };
  }, [unlocked, timeoutMs]);
}
