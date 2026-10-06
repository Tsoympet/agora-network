/**
 * Which transaction builders exist on this base.
 * OVL contract calls stay PLANNED: the OVL-EVM crate is not in this lineage,
 * and nonempty calldata is already rejected by the value-call preimage.
 */

import {
  buildAccountTransfer,
  buildDrcPayment,
  buildOvlValueCall,
} from "../light-client/envelopes";
import { buildSignedTransfer } from "../light-client/wallet";

export const OVL_CONTRACT_CALL = {
  status: "PLANNED" as const,
  reason:
    "OVL-EVM is not on this base. Contract calls and contract creation stay unavailable, and nonempty calldata is rejected.",
};

export const TX_BUILDERS = {
  drcPayment: { status: "present" as const, build: buildDrcPayment },
  ovlTransfer: { status: "present" as const, build: buildAccountTransfer },
  tltTransfer: { status: "present" as const, build: buildSignedTransfer },
  ovlValueCall: {
    status: "present" as const,
    build: buildOvlValueCall,
    note: "Empty calldata only. This is a value transfer, not a contract call.",
  },
  ovlContractCall: OVL_CONTRACT_CALL,
} as const;

export function buildOvlContractCall(): never {
  throw new Error(`PLANNED: ${OVL_CONTRACT_CALL.reason}`);
}
