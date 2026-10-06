/**
 * Community reward configuration.
 * TLT emission is never a funding source. This module does not mint.
 */

export type RewardKind =
  | "drc"
  | "ovl"
  | "tlt"
  | "badge"
  | "reputation"
  | "certificate"
  | "grant_eligibility"
  | "program_access"
  | "event_credential";

export type RewardSource =
  | { type: "emission" }
  | { type: "existing_treasury"; available_base_units: string; amount_base_units: string }
  | { type: "attestation" }
  | { type: "program_rule" };

export type RewardConfig = {
  kind: RewardKind;
  label: string;
  source: RewardSource;
};

export type RewardDecision = {
  kind: RewardKind;
  label: string;
  payable: boolean;
  ui_blocked: boolean;
  changes_tlt_emission: false;
  supply_delta_base_units: "0";
  funding_note: string;
  block_reason: string | null;
};

export type RewardControl = {
  disabled: boolean;
  label: "Blocked" | "Show funding path";
  submits_transaction: false;
};

export type TreasuryInputs = {
  drc_available: string;
  drc_amount: string;
  ovl_available: string;
  ovl_amount: string;
  tlt_available: string;
  tlt_amount: string;
};

const TLT_EMISSION_BLOCK =
  "TLT community rewards are not payable from PoW emission. The emission schedule is unchanged. This offer is blocked in the UI.";

const U64_MAX = 18446744073709551615n;

export function defaultTreasuryInputs(): TreasuryInputs {
  return {
    drc_available: "0",
    drc_amount: "1",
    ovl_available: "0",
    ovl_amount: "1",
    tlt_available: "0",
    tlt_amount: "1",
  };
}

export function rewardProgramFromInputs(input: TreasuryInputs): RewardConfig[] {
  return [
    treasury("drc", "DRC", input.drc_available, input.drc_amount),
    treasury("ovl", "OVL", input.ovl_available, input.ovl_amount),
    { kind: "tlt", label: "TLT from emission", source: { type: "emission" } },
    treasury("tlt", "TLT from existing treasury", input.tlt_available, input.tlt_amount),
    { kind: "badge", label: "Badge", source: { type: "attestation" } },
    { kind: "reputation", label: "Reputation", source: { type: "attestation" } },
    { kind: "certificate", label: "Certificate", source: { type: "attestation" } },
    { kind: "grant_eligibility", label: "Grant eligibility", source: { type: "program_rule" } },
    { kind: "program_access", label: "Program access", source: { type: "program_rule" } },
    { kind: "event_credential", label: "Event credential", source: { type: "attestation" } },
  ];
}

export function evaluateReward(config: RewardConfig): RewardDecision {
  if (!config.label.trim()) {
    return blocked(config, "reward label is empty", "");
  }
  const monetary = config.kind === "drc" || config.kind === "ovl" || config.kind === "tlt";
  if (config.source.type === "emission" && config.kind === "tlt") {
    return blocked(config, TLT_EMISSION_BLOCK, TLT_EMISSION_BLOCK);
  }
  if (config.source.type === "emission" && monetary) {
    return blocked(
      config,
      "DRC and OVL rewards are not payable from a new mint. Community programs do not inflate those assets.",
      "",
    );
  }
  if ((config.source.type === "emission" || config.source.type === "existing_treasury") && !monetary) {
    return blocked(
      config,
      "Badges, reputation, certificates, grant eligibility, program access, and event credentials are credentials. Emission and treasury spends do not fund them.",
      "",
    );
  }
  if ((config.source.type === "attestation" || config.source.type === "program_rule") && monetary) {
    return blocked(
      config,
      "DRC, OVL, and TLT rewards need an existing treasury balance of already-issued coins. An attestation does not mint.",
      "",
    );
  }
  if (config.source.type === "attestation" || config.source.type === "program_rule") {
    return open(config, "Credential only. No native asset moves and no supply changes.");
  }
  if (config.source.type !== "existing_treasury") {
    return blocked(config, "reward source is not payable", "");
  }
  const available = parseUnits(config.source.available_base_units);
  const amount = parseUnits(config.source.amount_base_units);
  if ("error" in available) return blocked(config, available.error, "");
  if ("error" in amount) return blocked(config, amount.error, "");
  if (amount.value === 0n) return blocked(config, "amount is zero", "");
  if (available.value < amount.value) {
    return blocked(
      config,
      "already-issued balance does not cover the reward. Emission is not used to fill the gap.",
      "",
    );
  }
  const note =
    config.kind === "tlt"
      ? "Transfer of already-issued TLT. The PoW emission schedule is unchanged. This decision does not mint."
      : config.kind === "drc"
        ? "Transfer of already-issued DRC. This decision does not mint."
        : "Transfer of already-issued OVL. This decision does not mint.";
  return open(config, note);
}

export function evaluateRewardProgram(configs: readonly RewardConfig[]): RewardDecision[] {
  return configs.map(evaluateReward);
}

export function rewardControl(decision: RewardDecision): RewardControl {
  const disabled = decision.ui_blocked || !decision.payable;
  return {
    disabled,
    label: disabled ? "Blocked" : "Show funding path",
    submits_transaction: false,
  };
}

function treasury(kind: RewardKind, label: string, available: string, amount: string): RewardConfig {
  return {
    kind,
    label,
    source: {
      type: "existing_treasury",
      available_base_units: available,
      amount_base_units: amount,
    },
  };
}

function blocked(config: RewardConfig, reason: string, note: string): RewardDecision {
  return {
    kind: config.kind,
    label: config.label,
    payable: false,
    ui_blocked: true,
    changes_tlt_emission: false,
    supply_delta_base_units: "0",
    funding_note: note,
    block_reason: reason,
  };
}

function open(config: RewardConfig, note: string): RewardDecision {
  return {
    kind: config.kind,
    label: config.label,
    payable: true,
    ui_blocked: false,
    changes_tlt_emission: false,
    supply_delta_base_units: "0",
    funding_note: note,
    block_reason: null,
  };
}

function parseUnits(value: string): { value: bigint } | { error: string } {
  if (!/^[0-9]+$/.test(value)) return { error: "base units must be a decimal string" };
  const parsed = BigInt(value);
  if (parsed > U64_MAX) return { error: "base units exceed u64" };
  return { value: parsed };
}
