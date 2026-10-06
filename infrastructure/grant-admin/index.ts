/**
 * Grant and mission management. Records eligibility and reviews.
 * It does not move treasury funds.
 */

export const GRANT_ADMIN_TRUST =
  "Operator-hosted grant and mission admin. Transitions and reviews are not consensus receipts and do not disburse treasury funds.";

export type MissionTransition = {
  id: string;
  from: string;
  to: string;
};

const NEXT: Record<string, string> = {
  AVAILABLE: "ACCEPTED",
  ACCEPTED: "IN_PROGRESS",
  IN_PROGRESS: "SUBMITTED",
  SUBMITTED: "COMPLETED",
};

export function createGrantAdmin<TGrant, TMission extends { id: string; state: string }>(input: {
  grants: readonly TGrant[];
  missions: readonly TMission[];
}) {
  const grants = [...input.grants];
  const missions = input.missions.map((mission) => ({ ...mission }));
  const reviews: { missionId: string; note: string }[] = [];
  return {
    trust: GRANT_ADMIN_TRUST,
    grants: () => grants,
    missions: () => missions,
    reviews: () => reviews,
    advance(id: string, to: string): { recorded: true } {
      const mission = missions.find((row) => row.id === id);
      if (!mission) throw new Error("unknown mission");
      if (NEXT[mission.state] !== to) {
        throw new Error(`illegal mission transition ${mission.state} → ${to}`);
      }
      mission.state = to;
      return { recorded: true };
    },
    review(missionId: string, note: string): { stored: true } {
      if (!missionId || !note.trim()) throw new Error("missionId and note required");
      reviews.push({ missionId, note });
      return { stored: true };
    },
  };
}

export type GrantAdmin = ReturnType<typeof createGrantAdmin>;
