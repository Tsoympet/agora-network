import { useEffect, useRef } from "react";

/** Phone lock timer measured from unlock. Native touch reset is not wired; the settings control changes the duration. */
export function useVaultSessionTimeout(
  unlocked: boolean,
  onLock: () => void,
  timeoutMs: number,
): void {
  const lockRef = useRef(onLock);
  lockRef.current = onLock;
  useEffect(() => {
    if (!unlocked || timeoutMs < 1_000) return;
    const timer = setTimeout(() => lockRef.current(), timeoutMs);
    return () => clearTimeout(timer);
  }, [unlocked, timeoutMs]);
}
