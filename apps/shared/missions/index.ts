import { transitionMission } from "../community/missions";
import type { MissionState } from "../community/types";

export { transitionMission };

/** Mission transitions do not create a payment. */
export function missionSettlement(state: MissionState): {
  state: MissionState;
  payment: null;
  onChain: "PLANNED";
} {
  return { state, payment: null, onChain: "PLANNED" };
}
