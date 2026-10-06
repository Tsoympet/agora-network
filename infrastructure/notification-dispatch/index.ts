/**
 * Dispatch loop for an operator. Device apps register a token over HTTP.
 * Payloads must not carry amounts or balances.
 */

export const NOTIFICATION_TRUST =
  "Operator-run notification dispatcher. Device apps do not run this loop. Payloads must not include amounts or balances. The operator can drop or delay a notice.";

const AMOUNT = /\b(amount|balance|base_units)\b/i;
const ASSET_AMOUNT = /\b\d[\d,]*\s*(DRC|TLT|OVL|base units)\b/i;

function rejectAmounts(value: unknown): void {
  const text = JSON.stringify(value);
  if (AMOUNT.test(text) || ASSET_AMOUNT.test(text)) {
    throw new Error("notification payload must not include amounts or balances");
  }
}

export function createNotificationDispatcher() {
  const registrations: { token: string }[] = [];
  const queue: { title: string; body: string }[] = [];
  return {
    trust: NOTIFICATION_TRUST,
    register(token: string) {
      rejectAmounts({ token });
      if (!token.trim()) throw new Error("device token required");
      registrations.push({ token });
      return { registered: true };
    },
    dispatch(input: { title: string; body: string }) {
      rejectAmounts(input);
      queue.push({ title: input.title, body: input.body });
      return { queued: queue.length };
    },
  };
}
