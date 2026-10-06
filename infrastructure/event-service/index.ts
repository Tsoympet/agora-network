/** Operator-hosted community events. Times and venues are not consensus. */

export const EVENT_TRUST =
  "Operator-hosted event list. Schedules are not chain transactions. The operator can change or remove an event.";

export function createEventService<T>(events: readonly T[]) {
  const rows = [...events];
  return {
    trust: EVENT_TRUST,
    list: () => rows,
  };
}
