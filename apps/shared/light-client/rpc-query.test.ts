/**
 * Shared light-client query wrappers.
 * Run: `node --experimental-strip-types light-client/rpc-query.test.ts`
 */
import assert from "node:assert/strict";

import { createLightClient } from "./rpc.ts";

const calls: Array<{ method: string; params: unknown }> = [];
const originalFetch = globalThis.fetch;
globalThis.fetch = (async (_url: unknown, init?: RequestInit) => {
  const body = JSON.parse(String(init?.body)) as {
    method: string;
    params: unknown;
  };
  calls.push({ method: body.method, params: body.params });
  return {
    ok: true,
    json: async () => ({ result: { ok: true, method: body.method } }),
  } as Response;
}) as typeof fetch;

try {
  const client = createLightClient({ rpcUrl: "http://127.0.0.1/rpc" });
  await client.getAccountBalances("agoradev1qqqq");
  await client.getTltCovenant("aa".repeat(32));
  await client.getDataCommitment({ authorization_id: "dd".repeat(32) });
  await client.getDrcOffer("bb".repeat(32));
  await client.getDrcAccountOffers({ account: "agoradev1qqqq", limit: 8 });
  await client.getDrcBookOffers({
    book: {
      pays: { type: "native_drc" },
      gets: {
        type: "issued",
        issuer: "aabbccddeeff00112233445566778899aabbccdd",
        currency: "USD",
      },
    },
    cursor: "c1",
  });
  await client.getDrcEscrow("cc".repeat(32));
  await client.getEthChainId();
  await client.getEthBalance("0x" + "11".repeat(20));
  await client.submitTltCovenant({ version: 2 });
  await client.submitAccountTransfer({ version: 2 });
  await client.submitOvlExecution({ version: 1 });
  await client.submitDrcPayment({ version: 4 });
  await client.submitDataCommitment({ version: 1 });
  await client.submitDrcOfferCreate({ version: 1 });
  await client.submitDrcTicketCreate({ version: 1 });
  await client.submitDrcEscrowCreate({ version: 1 });
  await client.sendRawEvmTransaction("02c0");
  await client.submitPassportAttestation({ version: 1 });
  await client.getPassportAttestation("ee".repeat(32));
  await client.getPassportIssuerNonce("agoradev1qqqq");
  await client.submitHubRegistration({ version: 1 });
  await client.getHubRegistration("ff".repeat(32));
  await client.getHubCoordinatorNonce("agoradev1qqqq");
  await client.submitGrantRegistration({ version: 1 });
  await client.getGrantRegistrarNonce("agoradev1qqqq");
  await client.submitMissionRegistration({ version: 1 });
  await client.getMissionSponsorNonce("agoradev1qqqq");
  await client.submitTreasuryDisbursement({ version: 1 });
  await client.getTreasuryDisbursement("aa".repeat(32));
  await client.getTreasuryNonce("ovl_builder");
  await client.submitVestingUnlock({ version: 1 });
  await client.getVestingUnlock("aa".repeat(32));
  await client.getVestingNonce("agoradev1qqqq");
  await client.getDrcPayment("11".repeat(32));
  await client.getDrcPaymentByInvoice({
    recipient: "agoradev1qqqq",
    invoice_id: "22".repeat(32),
  });
  await client.getDrcPaymentChannel("33".repeat(32));
  await client.getDrcPaymentChannelReceipt("33".repeat(32));
  await client.getDrcPaymentChannelFundEvent("44".repeat(32));
  await client.getDrcPaymentChannelClaimEvent("55".repeat(32));
  await client.getDrcPaymentChannelScheduleEvent("66".repeat(32));
  await client.verifyDrcPaymentChannelClaim({
    channel_id: "33".repeat(32),
    cumulative_authorized: 7,
    channel_claim_signature: "77".repeat(32),
  });
  await client.getDrcAccountPolicy("agoradev1qqqq");
  await client.getDrcAccountKeys("agoradev1qqqq");
  await client.getDrcAccountSignerList("agoradev1qqqq");
  await client.getDrcDepositPreauth({
    owner: "agoradev1qqqq",
    authorized_source: "agoradev1qqqq",
  });
  await client.getDrcEscrowReceipt("cc".repeat(32));
  await client.getDrcCheckReceipt("88".repeat(32));
  await client.getDrcIssuedTransferReceipt("99".repeat(32));
  await client.getDrcIssuedAssetPolicyReceipt("ab".repeat(32));
  await client.getDrcTrustLineIssuerControlReceipt("cd".repeat(32));
  await client.getDrcIssuedClawbackReceipt("ef".repeat(32));
  await client.getValidator("OVL", "agoradev1qqqq");
  await client.submitStakeTx({ version: 1, kind: "Bond" });

  assert.deepEqual(
    calls.map((call) => call.method),
    [
      "agora_getAccountBalances",
      "agora_getTltCovenant",
      "agora_getDataCommitment",
      "agora_getDrcOffer",
      "agora_getDrcAccountOffers",
      "agora_getDrcBookOffers",
      "agora_getDrcEscrow",
      "eth_chainId",
      "eth_getBalance",
      "agora_submitTltCovenant",
      "agora_submitAccountTransfer",
      "agora_submitOvlExecution",
      "agora_submitDrcPayment",
      "agora_submitDataCommitment",
      "agora_submitDrcOfferCreate",
      "agora_submitDrcTicketCreate",
      "agora_submitDrcEscrowCreate",
      "eth_sendRawTransaction",
      "agora_submitPassportAttestation",
      "agora_getPassportAttestation",
      "agora_getPassportIssuerNonce",
      "agora_submitHubRegistration",
      "agora_getHubRegistration",
      "agora_getHubCoordinatorNonce",
      "agora_submitGrantRegistration",
      "agora_getGrantRegistrarNonce",
      "agora_submitMissionRegistration",
      "agora_getMissionSponsorNonce",
      "agora_submitTreasuryDisbursement",
      "agora_getTreasuryDisbursement",
      "agora_getTreasuryNonce",
      "agora_submitVestingUnlock",
      "agora_getVestingUnlock",
      "agora_getVestingNonce",
      "agora_getDrcPayment",
      "agora_getDrcPaymentByInvoice",
      "agora_getDrcPaymentChannel",
      "agora_getDrcPaymentChannelReceipt",
      "agora_getDrcPaymentChannelFundEvent",
      "agora_getDrcPaymentChannelClaimEvent",
      "agora_getDrcPaymentChannelScheduleEvent",
      "agora_verifyDrcPaymentChannelClaim",
      "agora_getDrcAccountPolicy",
      "agora_getDrcAccountKeys",
      "agora_getDrcAccountSignerList",
      "agora_getDrcDepositPreauth",
      "agora_getDrcEscrowReceipt",
      "agora_getDrcCheckReceipt",
      "agora_getDrcIssuedTransferReceipt",
      "agora_getDrcIssuedAssetPolicyReceipt",
      "agora_getDrcTrustLineIssuerControlReceipt",
      "agora_getDrcIssuedClawbackReceipt",
      "agora_getValidator",
      "agora_submitStakeTx",
    ],
  );
  assert.deepEqual(calls[0].params, { address: "agoradev1qqqq" });
  assert.deepEqual(calls[1].params, { tx_id: "aa".repeat(32) });
  assert.deepEqual(calls[2].params, { authorization_id: "dd".repeat(32) });
  assert.deepEqual(calls[3].params, { offer_id: "bb".repeat(32) });
  assert.deepEqual(calls[4].params, {
    account: "agoradev1qqqq",
    limit: 8,
  });
  assert.deepEqual(calls[5].params, {
    pays: { type: "native_drc" },
    gets: {
      type: "issued",
      issuer: "aabbccddeeff00112233445566778899aabbccdd",
      currency: "USD",
    },
    cursor: "c1",
  });
  assert.deepEqual(calls[6].params, { escrow_id: "cc".repeat(32) });
  assert.deepEqual(calls[7].params, []);
  assert.deepEqual(calls[8].params, ["0x" + "11".repeat(20), "latest"]);
  assert.deepEqual(calls[9].params, { covenant: { version: 2 } });
  assert.deepEqual(calls[10].params, { account_transfer: { version: 2 } });
  assert.deepEqual(calls[11].params, { execution: { version: 1 } });
  assert.deepEqual(calls[12].params, { payment: { version: 4 } });
  assert.deepEqual(calls[13].params, { authorization: { version: 1 } });
  assert.deepEqual(calls[14].params, { offer_create: { version: 1 } });
  assert.deepEqual(calls[15].params, { ticket_create: { version: 1 } });
  assert.deepEqual(calls[16].params, { escrow_create: { version: 1 } });
  assert.deepEqual(calls[17].params, ["0x02c0"]);
  assert.deepEqual(calls[18].params, { attestation: { version: 1 } });
  assert.deepEqual(calls[19].params, { attestation_id: "ee".repeat(32) });
  assert.deepEqual(calls[20].params, { issuer: "agoradev1qqqq" });
  assert.deepEqual(calls[34].params, { payment_id: "11".repeat(32) });
  assert.deepEqual(calls[35].params, {
    recipient: "agoradev1qqqq",
    invoice_id: "22".repeat(32),
  });
  assert.deepEqual(calls[36].params, { channel_id: "33".repeat(32) });
  assert.deepEqual(calls[41].params, {
    channel_id: "33".repeat(32),
    cumulative_authorized: 7,
    channel_claim_signature: "77".repeat(32),
  });
  assert.deepEqual(calls[42].params, { account: "agoradev1qqqq" });
  assert.deepEqual(calls[45].params, {
    owner: "agoradev1qqqq",
    authorized_source: "agoradev1qqqq",
  });
  assert.deepEqual(calls[48].params, { transfer_tx_id: "99".repeat(32) });
  assert.deepEqual(calls[51].params, { clawback_tx_id: "ef".repeat(32) });
  assert.deepEqual(calls[52].params, { asset: "OVL", operator: "agoradev1qqqq" });
  assert.deepEqual(calls[53].params, { stake_tx: { version: 1, kind: "Bond" } });
  console.log("light-client query wrappers ok");
} finally {
  globalThis.fetch = originalFetch;
}
