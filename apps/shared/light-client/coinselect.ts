/**
 * Deterministic TLT coin selection.
 * Must match `core/crates/types/src/tlt_coinselect.rs`.
 * Wallet policy only: it does not change consensus validity.
 */

export const TLT_COINSELECT_EXHAUSTIVE_CAP = 12;

export type TltSpendCoin = {
  tx_id: string;
  index: number;
  value: number;
};

function coinKey(txId: string): string {
  return txId.trim().toLowerCase().replace(/^0x/, "");
}

function sortCoins(coins: TltSpendCoin[]): TltSpendCoin[] {
  return [...coins].sort((a, b) => {
    if (a.value !== b.value) return b.value - a.value;
    const left = coinKey(a.tx_id);
    const right = coinKey(b.tx_id);
    if (left !== right) return left < right ? -1 : 1;
    return a.index - b.index;
  });
}

function better(
  change: number,
  idxs: number[],
  best: { change: number; idxs: number[] } | null,
): boolean {
  if (!best) return true;
  if (change !== best.change) return change < best.change;
  if (idxs.length !== best.idxs.length) return idxs.length < best.idxs.length;
  for (let i = 0; i < idxs.length; i += 1) {
    if (idxs[i] !== best.idxs[i]) return idxs[i] < best.idxs[i];
  }
  return false;
}

function exhaustive(coins: TltSpendCoin[], target: number): TltSpendCoin[] | null {
  const n = coins.length;
  if (n === 0 || n > TLT_COINSELECT_EXHAUSTIVE_CAP) return null;
  let best: { change: number; idxs: number[] } | null = null;
  const limit = 1 << n;
  for (let mask = 1; mask < limit; mask += 1) {
    let sum = 0;
    const idxs: number[] = [];
    for (let i = 0; i < n; i += 1) {
      if ((mask & (1 << i)) === 0) continue;
      sum += coins[i].value;
      idxs.push(i);
    }
    if (sum < target) continue;
    const change = sum - target;
    if (better(change, idxs, best)) best = { change, idxs };
  }
  if (!best) return null;
  return best.idxs.map((i) => coins[i]);
}

function largestFirst(coins: TltSpendCoin[], target: number): TltSpendCoin[] {
  let sum = 0;
  const chosen: TltSpendCoin[] = [];
  for (const coin of coins) {
    sum += coin.value;
    chosen.push(coin);
    if (sum >= target) return chosen;
  }
  const have = coins.reduce((acc, coin) => acc + coin.value, 0);
  throw new Error(`insufficient funds: have ${have}, need ${target}`);
}

/** Select inputs covering `amount + fee` in base units. */
export function selectTltCoins(
  coins: TltSpendCoin[],
  amount: number,
  fee: number,
): TltSpendCoin[] {
  if (amount <= 0) throw new Error("amount must be > 0");
  const target = amount + fee;
  if (!Number.isSafeInteger(target)) throw new Error("amount plus fee overflows");
  const ordered = sortCoins(coins);
  const window = exhaustive(ordered.slice(0, TLT_COINSELECT_EXHAUSTIVE_CAP), target);
  if (window) return window;
  return largestFirst(ordered, target);
}
