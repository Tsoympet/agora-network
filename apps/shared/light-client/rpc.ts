/** Shared HTTP JSON-RPC helpers for Agora light clients (desktop / mobile / explorer). */

export type LightTxOut = {
  value: number;
  address: string;
};

export type LightTxIn = {
  tx_id: string;
  index: number;
};

export type LightTx = {
  tx_id: string;
  version: number;
  inputs: LightTxIn[];
  outputs: LightTxOut[];
  nonce: number;
  /** True when the transaction has no inputs (coinbase / mint). */
  is_coinbase: boolean;
};

export type LightBlock = {
  id: string;
  header: {
    version: number;
    parents: string[];
    timestamp_ms: number;
    bits: number;
    nonce: number;
    tx_root: string;
  };
  tx_count: number;
  transactions?: LightTx[];
};

export type LightUtxo = {
  tx_id: string;
  index: number;
  value: number;
};

export type LightBalance = {
  address: string;
  balance: number;
};

export type LightAccountBalances = {
  address: string;
  tlt: { balance: LightAmount };
  ovl: { balance: LightAmount; nonce: number };
  drc: { balance: LightAmount; nonce: number };
};

export type LightTltCovenantLookup = {
  tx_id: string;
  status: LightTxStatus;
  block_id: string | null;
  index: number | null;
  fee: number | null;
  confirmations: number | null;
  transaction: unknown | null;
};

export type LightDataCommitmentLookup = {
  authorization_id: string;
  status: string;
  block_id: string | null;
  index: number | null;
  confirmations: number | null;
  finalized: boolean;
  pow_work_met: boolean;
  acceptance: string | null;
  lane_enabled: boolean;
  canonical_l1?: boolean;
  lab_record_da?: boolean;
  authorization: unknown | null;
};

export type LightDrcOfferLookup = {
  offer_id?: string;
  status?: string;
  [key: string]: unknown;
};

export type LightDrcOfferPage = {
  offers?: unknown[];
  next_cursor?: unknown;
  [key: string]: unknown;
};

export type LightUtxoSet = {
  address: string;
  utxos: LightUtxo[];
};

export type SubmitTxResult = {
  tx_id: string;
};

export type LightTxStatus = "pending" | "confirmed" | "unknown";

export type LightTxLookup = {
  tx_id: string;
  status: LightTxStatus;
  block_id: string | null;
  index: number | null;
  fee: number | null;
  /** Blue-score / tip-depth confirmations when confirmed. */
  confirmations: number | null;
  transaction: LightTx | null;
};

export type LightMempoolEntry = {
  tx_id: string;
  fee: number | null;
  transaction: LightTx;
};

export type LightMempool = {
  count: number;
  transactions: LightMempoolEntry[];
};

export type LightNodeInfo = {
  network: string;
  version: string;
  peer_id: string | null;
  connected_peers: number | null;
  tip_count: number;
  mempool_count: number;
  pow_algorithm: string;
  bits: number;
  archival: boolean;
  hot_window: number;
  allow_fund: boolean;
  miner_address: string | null;
  /** Hex id of Block 0 when reported by the node. */
  genesis_hash: string | null;
  /** Network-bound signing chain id (`agora-testnet-1`, …). */
  chain_id?: string | null;
  /** Minimum mempool relay fee when reported. */
  min_relay_fee?: number;
};

/** JSON-safe base units. Nodes may encode large amounts as decimal strings. */
export type LightAmount = number | string;

export type NativeAssetTicker = "TLT" | "OVL" | "DRC";

export type LightFinality = {
  block_hash: string;
  blue_score?: number;
  state: string;
  pow_work_met: boolean;
  ovl_signed_stake?: LightAmount;
  ovl_active_stake?: LightAmount;
  drc_signed_stake?: LightAmount;
  drc_active_stake?: LightAmount;
  finalized: boolean;
  finalized_tip_blue_score: number;
};

export type LightValidatorSet = {
  asset: "OVL" | "DRC";
  epoch: number;
  total_active_stake: LightAmount;
  commitment: string;
  validators: Array<{
    operator: string;
    voting_power: LightAmount;
  }>;
};

export type LightRewardPool = {
  asset: "OVL" | "DRC";
  amount: LightAmount;
};

export type LightNativeAssetSupply = {
  asset: NativeAssetTicker;
  maximum_supply: string;
  issued_supply: string;
  burned_supply: string;
  net_supply: string;
};

export type LightProtocolTreasuries = {
  maturity: string;
  consensus_mutations_active: boolean;
  governance_root: string;
  policy: {
    version: number;
    constitution_id: string;
    constitution_hash: string;
    authorization_root: string;
  };
  treasuries: Array<{
    id: string;
    asset: NativeAssetTicker;
    balance: LightAmount;
    nonce?: number;
    controller?: string | null;
  }>;
};

export type LightCommunityRegistry = {
  maturity: string;
  consensus_mutations_active: boolean;
  root: string;
  counts: {
    hubs: number;
    passport_attestations: number;
    grants: number;
    missions: number;
  };
  hubs: unknown[];
  passport_attestations: unknown[];
  grants: unknown[];
  missions: unknown[];
};

export type FeeEstimate = {
  min_relay_fee: number;
  suggested_fee: number;
};

export type DrcLedgerObjectKind =
  | "account_policy"
  | "deposit_preauthorization"
  | "regular_key"
  | "signer_list"
  | "ticket_set"
  | "escrow"
  | "check"
  | "payment_channel"
  | "trust_line"
  | "issued_asset_policy";

/** Consensus hashes/addresses inside typed payloads use their generated byte-array shape. */
export type LightDrcLedgerObjectDescriptor = {
  version: number;
  object_id: number[];
  kind: DrcLedgerObjectKind;
  owner: number[];
  key: unknown;
  object: unknown;
};

export type LightDrcAccountObjects = {
  owner: number[];
  kind: DrcLedgerObjectKind | null;
  objects: LightDrcLedgerObjectDescriptor[];
  next_cursor: string | null;
};

export type LightDrcAcceptedOperationReceipt = {
  version: number;
  operation_id: number[];
  historical_transaction_id: number[];
  owner: number[];
  kind: string;
  canonical_block_id: number[];
  application_blue_score: number | null;
  operation: unknown;
  affected_object_ids: number[][];
};

export type VoteChoice = "yes" | "no" | "abstain" | "no_with_veto";

export type LightConstitution = {
  id: string;
  content_hash: string;
  body_markdown: string;
};

export type LightOffice = {
  rank: string;
  title: string;
  greek: string;
  seat_index: number;
  holder: string | null;
  elected_slot: number | null;
  term_end_slot: number | null;
};

export type LightProposalTally = {
  yes: number;
  no: number;
  abstain: number;
  no_with_veto: number;
};

export type LightProposal = {
  id: number;
  title: string;
  summary: string;
  kind: unknown;
  status: string;
  chamber: string;
  author: string;
  deposit: number;
  tally: LightProposalTally;
  sponsors?: string[];
  voting_start_slot?: number | null;
  voting_end_slot?: number | null;
};

export type LightProposalList = {
  count: number;
  proposals: LightProposal[];
};

export type LightGovernance = {
  constitution: LightConstitution;
  params: { min_deposit: number; [key: string]: unknown };
  offices: LightOffice[];
  proposal_count: number;
  topic_count: number;
  constitution_ack_count: number;
  ecclesia_eligible_power: number;
};

export type LightForumTopic = {
  id: number;
  author: string;
  title: string;
  body: string;
  category: string;
  created_slot: number;
  linked_proposal_id: number | null;
};

export type RpcStatus = "idle" | "ok" | "error";

export type LightClientConfig = {
  /** Full URL or path to JSON-RPC (`http://127.0.0.1:8545/rpc` or `/rpc`). */
  rpcUrl: string;
  /** Optional bearer token matching node `AGORA_RPC_TOKEN` (wallet / submit paths). */
  rpcToken?: string;
};

export type LightClient = {
  rpcUrl: string;
  call: <T>(method: string, params?: unknown) => Promise<T>;
  getDagTips: () => Promise<string[]>;
  getBlock: (hash: string) => Promise<LightBlock>;
  getTransaction: (txId: string) => Promise<LightTxLookup>;
  getMempool: (limit?: number) => Promise<LightMempool>;
  getNodeInfo: () => Promise<LightNodeInfo>;
  getFinality: (blockHash: string) => Promise<LightFinality>;
  getFinalizedTip: () => Promise<{ blue_score: number }>;
  getValidatorSet: (
    asset: "OVL" | "DRC",
    epoch?: number,
  ) => Promise<LightValidatorSet>;
  getRewardPool: (asset: "OVL" | "DRC") => Promise<LightRewardPool>;
  getNativeAssetSupply: (
    asset: NativeAssetTicker,
  ) => Promise<LightNativeAssetSupply>;
  getProtocolTreasuries: () => Promise<LightProtocolTreasuries>;
  getCommunityRegistry: (limit?: number) => Promise<LightCommunityRegistry>;
  getPassportAttestation: (attestationId: string) => Promise<{
    attestation_id: string;
    status: string;
    attestation: unknown | null;
  }>;
  getPassportIssuerNonce: (issuer: string) => Promise<{
    issuer: string;
    nonce: number;
  }>;
  getHubRegistration: (registrationId: string) => Promise<{
    registration_id: string;
    status: string;
    hub: unknown | null;
  }>;
  getHubCoordinatorNonce: (coordinator: string) => Promise<{
    coordinator: string;
    nonce: number;
  }>;
  getGrantRegistration: (registrationId: string) => Promise<{
    registration_id: string;
    status: string;
    grant: unknown | null;
  }>;
  getGrantRegistrarNonce: (registrar: string) => Promise<{
    registrar: string;
    nonce: number;
  }>;
  getMissionRegistration: (registrationId: string) => Promise<{
    registration_id: string;
    status: string;
    mission: unknown | null;
  }>;
  getMissionSponsorNonce: (sponsor: string) => Promise<{
    sponsor: string;
    nonce: number;
  }>;
  getTreasuryDisbursement: (disbursementId: string) => Promise<{
    disbursement_id: string;
    status: string;
    disbursement: unknown | null;
  }>;
  getTreasuryNonce: (treasury: string) => Promise<{
    treasury: string;
    nonce: number;
  }>;
  getDrcObject: (objectId: string) => Promise<{
    object_id: string;
    status: "live" | "unknown";
    object: LightDrcLedgerObjectDescriptor | null;
  }>;
  getDrcAccountObjects: (args: {
    account: string;
    kind?: DrcLedgerObjectKind;
    limit?: number;
    cursor?: string;
  }) => Promise<LightDrcAccountObjects>;
  getDrcOperation: (operationId: string) => Promise<{
    operation_id: string;
    status: "accepted" | "unknown";
    receipt: LightDrcAcceptedOperationReceipt | null;
  }>;
  getDrcTransaction: (transactionId: string) => Promise<{
    transaction_id: string;
    status: "accepted" | "unknown";
    receipt: LightDrcAcceptedOperationReceipt | null;
  }>;
  estimateFee: () => Promise<FeeEstimate>;
  getBalance: (address: string) => Promise<LightBalance>;
  getAccountBalances: (address: string) => Promise<LightAccountBalances>;
  getTltCovenant: (txId: string) => Promise<LightTltCovenantLookup>;
  getDataCommitment: (args: {
    authorization_id?: string;
    source?: unknown;
    sequence?: number;
  }) => Promise<LightDataCommitmentLookup>;
  getDrcOffer: (offerId: string) => Promise<LightDrcOfferLookup>;
  getDrcAccountOffers: (args: {
    account: string;
    limit?: number;
    cursor?: unknown;
  }) => Promise<LightDrcOfferPage>;
  getDrcBookOffers: (args: {
    book: unknown;
    limit?: number;
    cursor?: unknown;
  }) => Promise<LightDrcOfferPage>;
  getDrcEscrow: (escrowId: string) => Promise<unknown>;
  getDrcCheck: (checkId: string) => Promise<unknown>;
  getDrcTicket: (args: {
    owner: string;
    ticket_sequence: number;
  }) => Promise<unknown>;
  getDrcTrustLine: (args: {
    holder: string;
    issuer: string;
    currency: unknown;
  }) => Promise<unknown>;
  getEthChainId: () => Promise<unknown>;
  getEthBlockNumber: () => Promise<unknown>;
  getEthBalance: (address: string, blockTag?: string) => Promise<unknown>;
  sendRawEvmTransaction: (rawHex: string) => Promise<string>;
  getUtxos: (address: string) => Promise<LightUtxoSet>;
  /** Submit a signed transaction JSON body (native serde / byte-array hashes). */
  submitTransaction: (tx: unknown) => Promise<SubmitTxResult>;
  submitTltCovenant: (tx: unknown) => Promise<{ tx_id: string }>;
  submitAccountTransfer: (tx: unknown) => Promise<{ account_tx_id: string }>;
  submitOvlExecution: (tx: unknown) => Promise<{ execution_tx_id: string }>;
  submitDrcPayment: (tx: unknown) => Promise<{ payment_id: string }>;
  submitDataCommitment: (authorization: unknown) => Promise<{
    authorization_id: string;
  }>;
  submitDrcOfferCreate: (tx: unknown) => Promise<{
    offer_id: string;
    simulated_fill: boolean;
  }>;
  submitDrcOfferCancel: (tx: unknown) => Promise<{ cancel_tx_id: string }>;
  submitDrcEscrowCreate: (tx: unknown) => Promise<{ escrow_id: string }>;
  submitDrcEscrowFinish: (tx: unknown) => Promise<{ finish_tx_id: string }>;
  submitDrcEscrowCancel: (tx: unknown) => Promise<{ cancel_tx_id: string }>;
  submitDrcCheckCreate: (tx: unknown) => Promise<{ check_id: string }>;
  submitDrcCheckCash: (tx: unknown) => Promise<{ cash_tx_id: string }>;
  submitDrcCheckCancel: (tx: unknown) => Promise<{ cancel_tx_id: string }>;
  submitDrcPaymentChannelCreate: (tx: unknown) => Promise<{ channel_id: string }>;
  submitDrcPaymentChannelFund: (tx: unknown) => Promise<{ fund_tx_id: string }>;
  submitDrcPaymentChannelClaim: (tx: unknown) => Promise<{ claim_tx_id: string }>;
  submitDrcPaymentChannelClose: (tx: unknown) => Promise<{ close_tx_id: string }>;
  submitDrcTicketCreate: (tx: unknown) => Promise<{ ticket_create_tx_id: string }>;
  submitDrcRegularKey: (tx: unknown) => Promise<{ regular_key_tx_id: string }>;
  submitDrcSignerList: (tx: unknown) => Promise<{ signer_list_tx_id: string }>;
  submitDrcDepositPreauth: (tx: unknown) => Promise<{ preauth_tx_id: string }>;
  submitDrcAccountPolicy: (tx: unknown) => Promise<{ policy_tx_id: string }>;
  submitDrcTrustLineSet: (tx: unknown) => Promise<{ trust_line_set_tx_id: string }>;
  submitDrcIssuedTransfer: (tx: unknown) => Promise<{ issued_transfer_tx_id: string }>;
  submitDrcIssuedAssetPolicySet: (tx: unknown) => Promise<{ policy_set_tx_id: string }>;
  submitDrcTrustLineIssuerControl: (tx: unknown) => Promise<{
    issuer_control_tx_id: string;
  }>;
  submitDrcIssuedClawback: (tx: unknown) => Promise<{ clawback_tx_id: string }>;
  submitPassportAttestation: (attestation: unknown) => Promise<{
    attestation_id: string;
  }>;
  submitHubRegistration: (registration: unknown) => Promise<{
    registration_id: string;
  }>;
  submitGrantRegistration: (registration: unknown) => Promise<{
    registration_id: string;
  }>;
  submitMissionRegistration: (registration: unknown) => Promise<{
    registration_id: string;
  }>;
  submitTreasuryDisbursement: (disbursement: unknown) => Promise<{
    disbursement_id: string;
  }>;
  getConstitution: () => Promise<LightConstitution>;
  getGovernance: () => Promise<LightGovernance>;
  listProposals: (limit?: number) => Promise<LightProposalList>;
  getProposal: (id: number) => Promise<LightProposal>;
  listOffices: () => Promise<{ offices: LightOffice[] }>;
  listForumTopics: (limit?: number) => Promise<{ count: number; topics: LightForumTopic[] }>;
  castGovVote: (args: {
    id: number;
    voter: string;
    choice: VoteChoice;
    raw_balance?: number;
    total_supply?: number;
  }) => Promise<{ proposal_id: number; voted: boolean }>;
  submitProposal: (args: {
    author: string;
    title: string;
    summary: string;
    kind: unknown;
    slot?: number;
  }) => Promise<{ proposal_id: number }>;
  ackConstitution: (address: string, slot?: number) => Promise<{
    address: string;
    constitution_id: string;
    constitution_hash: string;
    acked: boolean;
  }>;
};

export function createLightClient(config: LightClientConfig): LightClient {
  let nextId = 1;
  const rpcUrl = config.rpcUrl;
  const rpcToken = config.rpcToken?.trim() || undefined;

  async function call<T>(method: string, params: unknown = []): Promise<T> {
    const headers: Record<string, string> = {
      "content-type": "application/json",
    };
    if (rpcToken) {
      headers.authorization = `Bearer ${rpcToken}`;
    }
    const res = await fetch(rpcUrl, {
      method: "POST",
      headers,
      body: JSON.stringify({ id: nextId++, method, params }),
    });
    if (!res.ok) {
      throw new Error(`RPC HTTP ${res.status}`);
    }
    const body = (await res.json()) as {
      result?: T;
      error?: { message?: string };
    };
    if (body.error) {
      throw new Error(body.error.message || "RPC error");
    }
    if (body.result === undefined) {
      throw new Error("RPC missing result");
    }
    return body.result;
  }

  return {
    rpcUrl,
    call,
    getDagTips: () => call<string[]>("agora_getDagTips", []),
    getBlock: (hash: string) => call<LightBlock>("agora_getBlock", { hash }),
    getTransaction: (txId: string) =>
      call<LightTxLookup>("agora_getTransaction", { tx_id: txId }),
    getMempool: (limit = 128) =>
      call<LightMempool>("agora_getMempool", { limit }),
    getNodeInfo: () => call<LightNodeInfo>("agora_getNodeInfo", []),
    getFinality: (blockHash: string) =>
      call<LightFinality>("agora_getFinality", { hash: blockHash }),
    getFinalizedTip: () =>
      call<{ blue_score: number }>("agora_getFinalizedTip", []),
    getValidatorSet: (asset, epoch) =>
      call<LightValidatorSet>("agora_getValidatorSet", {
        asset,
        ...(epoch === undefined ? {} : { epoch }),
      }),
    getRewardPool: (asset) =>
      call<LightRewardPool>("agora_getRewardPool", { asset }),
    getNativeAssetSupply: (asset) =>
      call<LightNativeAssetSupply>("agora_getNativeAssetSupply", { asset }),
    getProtocolTreasuries: () =>
      call<LightProtocolTreasuries>("agora_getProtocolTreasuries", []),
    getCommunityRegistry: (limit = 64) =>
      call<LightCommunityRegistry>("agora_getCommunityRegistry", { limit }),
    getPassportAttestation: (attestationId) =>
      call<{
        attestation_id: string;
        status: string;
        attestation: unknown | null;
      }>("agora_getPassportAttestation", { attestation_id: attestationId }),
    getPassportIssuerNonce: (issuer) =>
      call<{ issuer: string; nonce: number }>(
        "agora_getPassportIssuerNonce",
        { issuer },
      ),
    getHubRegistration: (registrationId) =>
      call<{
        registration_id: string;
        status: string;
        hub: unknown | null;
      }>("agora_getHubRegistration", { registration_id: registrationId }),
    getHubCoordinatorNonce: (coordinator) =>
      call<{ coordinator: string; nonce: number }>(
        "agora_getHubCoordinatorNonce",
        { coordinator },
      ),
    getGrantRegistration: (registrationId) =>
      call<{
        registration_id: string;
        status: string;
        grant: unknown | null;
      }>("agora_getGrantRegistration", { registration_id: registrationId }),
    getGrantRegistrarNonce: (registrar) =>
      call<{ registrar: string; nonce: number }>(
        "agora_getGrantRegistrarNonce",
        { registrar },
      ),
    getMissionRegistration: (registrationId) =>
      call<{
        registration_id: string;
        status: string;
        mission: unknown | null;
      }>("agora_getMissionRegistration", { registration_id: registrationId }),
    getMissionSponsorNonce: (sponsor) =>
      call<{ sponsor: string; nonce: number }>(
        "agora_getMissionSponsorNonce",
        { sponsor },
      ),
    getTreasuryDisbursement: (disbursementId) =>
      call<{
        disbursement_id: string;
        status: string;
        disbursement: unknown | null;
      }>("agora_getTreasuryDisbursement", { disbursement_id: disbursementId }),
    getTreasuryNonce: (treasury) =>
      call<{ treasury: string; nonce: number }>("agora_getTreasuryNonce", {
        treasury,
      }),
    getDrcObject: (objectId) =>
      call<{
        object_id: string;
        status: "live" | "unknown";
        object: LightDrcLedgerObjectDescriptor | null;
      }>("agora_getDrcObject", { object_id: objectId }),
    getDrcAccountObjects: (args) =>
      call<LightDrcAccountObjects>("agora_getDrcAccountObjects", {
        account: args.account,
        ...(args.kind === undefined ? {} : { kind: args.kind }),
        ...(args.limit === undefined ? {} : { limit: args.limit }),
        ...(args.cursor === undefined ? {} : { cursor: args.cursor }),
      }),
    getDrcOperation: (operationId) =>
      call<{
        operation_id: string;
        status: "accepted" | "unknown";
        receipt: LightDrcAcceptedOperationReceipt | null;
      }>("agora_getDrcOperation", { operation_id: operationId }),
    getDrcTransaction: (transactionId) =>
      call<{
        transaction_id: string;
        status: "accepted" | "unknown";
        receipt: LightDrcAcceptedOperationReceipt | null;
      }>("agora_getDrcTransaction", { transaction_id: transactionId }),
    estimateFee: () => call<FeeEstimate>("agora_estimateFee", []),
    getBalance: (address: string) =>
      call<LightBalance>("agora_getBalance", { address }),
    getAccountBalances: (address: string) =>
      call<LightAccountBalances>("agora_getAccountBalances", { address }),
    getTltCovenant: (txId: string) =>
      call<LightTltCovenantLookup>("agora_getTltCovenant", { tx_id: txId }),
    getDataCommitment: (args) =>
      call<LightDataCommitmentLookup>("agora_getDataCommitment", {
        ...(args.authorization_id === undefined
          ? {}
          : { authorization_id: args.authorization_id }),
        ...(args.source === undefined ? {} : { source: args.source }),
        ...(args.sequence === undefined ? {} : { sequence: args.sequence }),
      }),
    getDrcOffer: (offerId: string) =>
      call<LightDrcOfferLookup>("agora_getDrcOffer", { offer_id: offerId }),
    getDrcAccountOffers: (args) =>
      call<LightDrcOfferPage>("agora_getDrcAccountOffers", {
        account: args.account,
        ...(args.limit === undefined ? {} : { limit: args.limit }),
        ...(args.cursor === undefined ? {} : { cursor: args.cursor }),
      }),
    getDrcBookOffers: (args) =>
      call<LightDrcOfferPage>("agora_getDrcBookOffers", {
        ...(typeof args.book === "object" && args.book !== null
          ? (args.book as Record<string, unknown>)
          : { book: args.book }),
        ...(args.limit === undefined ? {} : { limit: args.limit }),
        ...(args.cursor === undefined ? {} : { cursor: args.cursor }),
      }),
    getDrcEscrow: (escrowId) =>
      call("agora_getDrcEscrow", { escrow_id: escrowId }),
    getDrcCheck: (checkId) => call("agora_getDrcCheck", { check_id: checkId }),
    getDrcTicket: (args) =>
      call("agora_getDrcTicket", {
        owner: args.owner,
        ticket_sequence: args.ticket_sequence,
      }),
    getDrcTrustLine: (args) =>
      call("agora_getDrcTrustLine", {
        holder: args.holder,
        issuer: args.issuer,
        currency: args.currency,
      }),
    getEthChainId: () => call("eth_chainId", []),
    getEthBlockNumber: () => call("eth_blockNumber", []),
    getEthBalance: (address, blockTag = "latest") =>
      call("eth_getBalance", [address, blockTag]),
    sendRawEvmTransaction: (rawHex) => {
      const hex = rawHex.startsWith("0x") || rawHex.startsWith("0X") ? rawHex : `0x${rawHex}`;
      return call<string>("eth_sendRawTransaction", [hex]);
    },
    getUtxos: (address: string) =>
      call<LightUtxoSet>("agora_getUtxos", { address }),
    submitTransaction: (tx: unknown) =>
      call<SubmitTxResult>("agora_submitTransaction", { tx }),
    submitTltCovenant: (tx) =>
      call<{ tx_id: string }>("agora_submitTltCovenant", { covenant: tx }),
    submitAccountTransfer: (tx) =>
      call<{ account_tx_id: string }>("agora_submitAccountTransfer", {
        account_transfer: tx,
      }),
    submitOvlExecution: (tx) =>
      call<{ execution_tx_id: string }>("agora_submitOvlExecution", {
        execution: tx,
      }),
    submitDrcPayment: (tx) =>
      call<{ payment_id: string }>("agora_submitDrcPayment", { payment: tx }),
    submitDataCommitment: (authorization) =>
      call<{ authorization_id: string }>("agora_submitDataCommitment", {
        authorization,
      }),
    submitDrcOfferCreate: (tx) =>
      call<{ offer_id: string; simulated_fill: boolean }>(
        "agora_submitDrcOfferCreate",
        { offer_create: tx },
      ),
    submitDrcOfferCancel: (tx) =>
      call<{ cancel_tx_id: string }>("agora_submitDrcOfferCancel", {
        offer_cancel: tx,
      }),
    submitDrcEscrowCreate: (tx) =>
      call<{ escrow_id: string }>("agora_submitDrcEscrowCreate", {
        escrow_create: tx,
      }),
    submitDrcEscrowFinish: (tx) =>
      call<{ finish_tx_id: string }>("agora_submitDrcEscrowFinish", {
        escrow_finish: tx,
      }),
    submitDrcEscrowCancel: (tx) =>
      call<{ cancel_tx_id: string }>("agora_submitDrcEscrowCancel", {
        escrow_cancel: tx,
      }),
    submitDrcCheckCreate: (tx) =>
      call<{ check_id: string }>("agora_submitDrcCheckCreate", {
        check_create: tx,
      }),
    submitDrcCheckCash: (tx) =>
      call<{ cash_tx_id: string }>("agora_submitDrcCheckCash", {
        check_cash: tx,
      }),
    submitDrcCheckCancel: (tx) =>
      call<{ cancel_tx_id: string }>("agora_submitDrcCheckCancel", {
        check_cancel: tx,
      }),
    submitDrcPaymentChannelCreate: (tx) =>
      call<{ channel_id: string }>("agora_submitDrcPaymentChannelCreate", {
        payment_channel_create: tx,
      }),
    submitDrcPaymentChannelFund: (tx) =>
      call<{ fund_tx_id: string }>("agora_submitDrcPaymentChannelFund", {
        payment_channel_fund: tx,
      }),
    submitDrcPaymentChannelClaim: (tx) =>
      call<{ claim_tx_id: string }>("agora_submitDrcPaymentChannelClaim", {
        payment_channel_claim: tx,
      }),
    submitDrcPaymentChannelClose: (tx) =>
      call<{ close_tx_id: string }>("agora_submitDrcPaymentChannelClose", {
        payment_channel_close: tx,
      }),
    submitDrcTicketCreate: (tx) =>
      call<{ ticket_create_tx_id: string }>("agora_submitDrcTicketCreate", {
        ticket_create: tx,
      }),
    submitDrcRegularKey: (tx) =>
      call<{ regular_key_tx_id: string }>("agora_submitDrcRegularKey", {
        regular_key: tx,
      }),
    submitDrcSignerList: (tx) =>
      call<{ signer_list_tx_id: string }>("agora_submitDrcSignerList", {
        signer_list: tx,
      }),
    submitDrcDepositPreauth: (tx) =>
      call<{ preauth_tx_id: string }>("agora_submitDrcDepositPreauth", {
        preauth: tx,
      }),
    submitDrcAccountPolicy: (tx) =>
      call<{ policy_tx_id: string }>("agora_submitDrcAccountPolicy", {
        policy: tx,
      }),
    submitDrcTrustLineSet: (tx) =>
      call<{ trust_line_set_tx_id: string }>("agora_submitDrcTrustLineSet", {
        trust_line_set: tx,
      }),
    submitDrcIssuedTransfer: (tx) =>
      call<{ issued_transfer_tx_id: string }>("agora_submitDrcIssuedTransfer", {
        issued_transfer: tx,
      }),
    submitDrcIssuedAssetPolicySet: (tx) =>
      call<{ policy_set_tx_id: string }>("agora_submitDrcIssuedAssetPolicySet", {
        issued_asset_policy_set: tx,
      }),
    submitDrcTrustLineIssuerControl: (tx) =>
      call<{ issuer_control_tx_id: string }>(
        "agora_submitDrcTrustLineIssuerControl",
        { trust_line_issuer_control: tx },
      ),
    submitDrcIssuedClawback: (tx) =>
      call<{ clawback_tx_id: string }>("agora_submitDrcIssuedClawback", {
        issued_clawback: tx,
      }),
    submitPassportAttestation: (attestation) =>
      call<{ attestation_id: string }>("agora_submitPassportAttestation", {
        attestation,
      }),
    submitHubRegistration: (registration) =>
      call<{ registration_id: string }>("agora_submitHubRegistration", {
        registration,
      }),
    submitGrantRegistration: (registration) =>
      call<{ registration_id: string }>("agora_submitGrantRegistration", {
        registration,
      }),
    submitMissionRegistration: (registration) =>
      call<{ registration_id: string }>("agora_submitMissionRegistration", {
        registration,
      }),
    submitTreasuryDisbursement: (disbursement) =>
      call<{ disbursement_id: string }>("agora_submitTreasuryDisbursement", {
        disbursement,
      }),
    getConstitution: () => call<LightConstitution>("agora_getConstitution", []),
    getGovernance: () => call<LightGovernance>("agora_getGovernance", []),
    listProposals: (limit = 64) =>
      call<LightProposalList>("agora_listProposals", { limit }),
    getProposal: (id: number) =>
      call<LightProposal>("agora_getProposal", { id }),
    listOffices: () =>
      call<{ offices: LightOffice[] }>("agora_listOffices", []),
    listForumTopics: (limit = 64) =>
      call<{ count: number; topics: LightForumTopic[] }>(
        "agora_listForumTopics",
        { limit },
      ),
    castGovVote: (args) =>
      call("agora_castGovVote", {
        id: args.id,
        voter: args.voter,
        choice: args.choice,
        raw_balance: args.raw_balance ?? 0,
        total_supply: args.total_supply ?? 1,
      }),
    submitProposal: (args) =>
      call("agora_submitProposal", {
        author: args.author,
        title: args.title,
        summary: args.summary,
        kind: args.kind,
        slot: args.slot ?? 0,
      }),
    ackConstitution: (address, slot = 0) =>
      call("agora_ackConstitution", { address, slot }),
  };
}
