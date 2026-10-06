import { MISSION_STATES, type MissionState } from "./types.ts";

const NEXT: Record<MissionState, MissionState | null> = {
  AVAILABLE: "ACCEPTED",
  ACCEPTED: "IN_PROGRESS",
  IN_PROGRESS: "SUBMITTED",
  SUBMITTED: "COMPLETED",
  COMPLETED: null,
};

/** Linear mission lifecycle. Skips and reversals fail closed. */
export function transitionMission(state: MissionState, to: MissionState): MissionState {
  if (!MISSION_STATES.includes(state) || !MISSION_STATES.includes(to)) {
    throw new Error("unknown mission state");
  }
  if (NEXT[state] !== to) {
    throw new Error(`illegal mission transition ${state} → ${to}`);
  }
  return to;
}
