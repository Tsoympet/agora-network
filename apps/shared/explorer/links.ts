/**
 * Cross-links into the explorer sections that exist today: #tx, #live, #trident.
 * Missing objects stay PLANNED. This module does not invent transaction ids.
 */

export type ChainLink = {
  label: string;
  status: "present" | "PLANNED";
  href: string | null;
  reason?: string;
};

const HEX64 = /^[0-9a-fA-F]{64}$/;

function txLink(asset: "DRC" | "OVL" | "TLT", txId: string | null | undefined): ChainLink {
  if (!txId || !HEX64.test(txId)) {
    return {
      label: `${asset} transaction`,
      status: "PLANNED",
      href: null,
      reason: "No verified transaction id is available.",
    };
  }
  return {
    label: `${asset} transaction`,
    status: "present",
    href: "#tx",
    reason: "Opens the explorer transaction section. Loading this id automatically is PLANNED.",
  };
}

export function assetTxLink(asset: "DRC" | "OVL" | "TLT", txId: string | null | undefined): ChainLink {
  return txLink(asset, txId);
}

export function blockLink(blockId: string | null | undefined): ChainLink {
  if (!blockId || !HEX64.test(blockId)) {
    return { label: "block", status: "PLANNED", href: null, reason: "No block id." };
  }
  return {
    label: "block",
    status: "present",
    href: "#live",
    reason: "Opens the live DAG section. Selecting this block automatically is PLANNED.",
  };
}

export function accountLink(address: string | null | undefined): ChainLink {
  if (!address || !address.trim()) {
    return { label: "account", status: "PLANNED", href: null, reason: "No account id." };
  }
  return {
    label: "account",
    status: "present",
    href: "#tx",
    reason: "The explorer can look up an address from the transaction section. A dedicated account page is PLANNED.",
  };
}

export function plannedMarketLink(
  kind: "contract" | "token" | "nft" | "dex" | "amm",
): ChainLink {
  return {
    label: kind,
    status: "PLANNED",
    href: null,
    reason: "This explorer has no contract, token, NFT, DEX, or AMM page, and OVL-EVM is not on this base.",
  };
}

export function grantTrail(input: {
  grantId?: string | null;
  fundingTxId?: string | null;
  treasuryId?: string | null;
  recipient?: string | null;
  passportAddress?: string | null;
}): ChainLink[] {
  return [
    {
      label: "grant",
      status: input.grantId ? "present" : "PLANNED",
      href: null,
      reason: input.grantId
        ? "Grant id is a community record. It does not disburse funds from this client."
        : "No grant id.",
    },
    txLink("DRC", input.fundingTxId),
    {
      label: "treasury",
      status: input.treasuryId ? "present" : "PLANNED",
      href: input.treasuryId ? "#trident" : null,
      reason: input.treasuryId
        ? "Opens the Trident treasury section. A grant-to-treasury disbursement proof is PLANNED."
        : "No treasury id, and grant disbursement is not active.",
    },
    {
      label: "recipient",
      status: input.recipient ? "present" : "PLANNED",
      href: null,
      reason: input.recipient ? "Recipient is an address on the grant record, not a payment receipt." : "No recipient.",
    },
    {
      label: "passport",
      status: input.passportAddress ? "present" : "PLANNED",
      href: null,
      reason: input.passportAddress
        ? "Passport screen for this address. Chain inclusion of attestations is PLANNED."
        : "No passport address.",
    },
  ];
}

export function merchantTrail(input: {
  drcAddress?: string | null;
  paymentTxId?: string | null;
  profileId?: string | null;
}): ChainLink[] {
  return [
    {
      label: "merchant profile",
      status: "PLANNED",
      href: null,
      reason: input.profileId
        ? "A profile id is not merchant activity. This client does not confirm sales."
        : "No merchant profile is confirmed.",
    },
    accountLink(input.drcAddress),
    txLink("DRC", input.paymentTxId),
  ];
}

export function developerTrail(input: {
  passportAddress?: string | null;
  projectId?: string | null;
  contractId?: string | null;
  grantId?: string | null;
  missionId?: string | null;
}): ChainLink[] {
  return [
    {
      label: "passport",
      status: input.passportAddress ? "present" : "PLANNED",
      href: null,
      reason: input.passportAddress ? "Passport address only." : "No passport address.",
    },
    {
      label: "projects",
      status: "PLANNED",
      href: null,
      reason: input.projectId ? "Project pages are not on this base." : "No project id.",
    },
    plannedMarketLink("contract"),
    {
      label: "grants",
      status: input.grantId ? "present" : "PLANNED",
      href: null,
      reason: input.grantId ? "Grant id is a community record, not a disbursement." : "No grant id.",
    },
    {
      label: "missions",
      status: input.missionId ? "present" : "PLANNED",
      href: null,
      reason: input.missionId ? "Mission id is a community record, not a payment." : "No mission id.",
    },
  ];
}

export function minerTrail(input: {
  statsId?: string | null;
  tltTxId?: string | null;
}): ChainLink[] {
  return [
    {
      label: "miner public stats",
      status: "PLANNED",
      href: null,
      reason: "The explorer has no miner stats page.",
    },
    txLink("TLT", input.tltTxId),
  ];
}
