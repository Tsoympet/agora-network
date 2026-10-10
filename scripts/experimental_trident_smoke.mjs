#!/usr/bin/env node
/**
 * Experimental Trident mesh smoke (generated keys 0xa1/0xa2/0xa3).
 *
 *   node --experimental-strip-types scripts/experimental_trident_smoke.mjs tx
 *   node --experimental-strip-types scripts/experimental_trident_smoke.mjs finality
 */
import { createLightClient } from "../apps/shared/light-client/rpc.ts";
import { accountFromSecretHex, sendTransferFromAccount } from "../apps/shared/light-client/wallet.ts";
import { sendCheckpointAttestation } from "../apps/shared/light-client/typed-lanes-finality.ts";

const TLT_SECRET = "a3".repeat(32);
const OVL_SECRET = "a1".repeat(32);
const DRC_SECRET = "a2".repeat(32);

const rpcA = process.env.AGORA_RPC_A || "http://127.0.0.1:8555/rpc";
const rpcB = process.env.AGORA_RPC_B || "http://127.0.0.1:8556/rpc";
const timeoutSecs = Number(process.env.AGORA_SMOKE_TIMEOUT_SECS || 60);
const amount = Number(process.env.AGORA_SMOKE_AMOUNT || 1);
const fee = Number(process.env.AGORA_SMOKE_FEE || 1);

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function smokeTx() {
  const from = accountFromSecretHex(TLT_SECRET, "testnet");
  const to = accountFromSecretHex(OVL_SECRET, "testnet");
  console.log(`from  ${from.addressBech32} ${from.addressHex}`);
  console.log(`to    ${to.addressBech32}`);
  console.log(`rpc A ${rpcA}`);
  console.log(`rpc B ${rpcB}`);

  const clientA = createLightClient({ rpcUrl: rpcA });
  const clientB = createLightClient({ rpcUrl: rpcB });
  const info = await clientA.getNodeInfo();
  console.log(
    `node-a genesis=${info.genesis_hash} chain_id=${info.chain_id} pow=${info.pow_algorithm} bits=${info.bits}`,
  );

  const { tx_id } = await sendTransferFromAccount(clientA, {
    secretHex: TLT_SECRET,
    toAddressHex: to.addressHex,
    amount,
    fee,
    network: "testnet",
  });
  console.log(`submitted on A: ${tx_id}`);

  const deadline = Date.now() + timeoutSecs * 1000;
  while (Date.now() < deadline) {
    const lookup = await clientB.getTransaction(tx_id);
    let inPool = false;
    try {
      const pool = await clientB.getMempool(128);
      inPool = pool.transactions.some((t) => t.tx_id === tx_id);
      console.log(`  B status=${lookup.status} in_pool=${inPool} count=${pool.count}`);
    } catch {
      console.log(`  B status=${lookup.status} mempool=n/a`);
    }
    if (lookup.status === "pending" || inPool) {
      console.log(`tx gossip OK — ${tx_id} pending on B`);
      return;
    }
    await sleep(1000);
  }
  throw new Error(`timed out waiting for ${tx_id} on B`);
}

async function smokeFinality() {
  const clientA = createLightClient({ rpcUrl: rpcA });
  const clientB = createLightClient({ rpcUrl: rpcB });
  const tips = await clientA.getDagTips();
  if (!tips.length) throw new Error("node-a has no tips");
  const tip = tips[0];
  console.log(`attesting tip ${tip}`);
  const before = await clientA.getFinality(tip);
  console.log(
    `before state=${before.state} pow=${before.pow_work_met} finalized=${before.finalized} body=${Boolean(before.body)}`,
  );
  if (!before.body) {
    throw new Error("getFinality missing checkpoint body — cannot sign");
  }

  const ovl = await sendCheckpointAttestation(clientA, {
    secretHex: OVL_SECRET,
    set: "OVL",
    blockHash: tip,
    network: "testnet",
  });
  console.log(`OVL attestation: state=${ovl.state} finalized=${ovl.finalized}`);
  const drc = await sendCheckpointAttestation(clientA, {
    secretHex: DRC_SECRET,
    set: "DRC",
    blockHash: tip,
    network: "testnet",
  });
  console.log(`DRC attestation: state=${drc.state} finalized=${drc.finalized}`);

  const deadline = Date.now() + timeoutSecs * 1000;
  while (Date.now() < deadline) {
    const fa = await clientA.getFinality(tip);
    let fb;
    try {
      fb = await clientB.getFinality(tip);
    } catch {
      fb = null;
    }
    console.log(
      `  A finalized=${fa.finalized} state=${fa.state} | B finalized=${fb?.finalized ?? "n/a"} state=${fb?.state ?? "?"}`,
    );
    if (fa.finalized && fb?.finalized) {
      console.log("dual-PoS finality OK on both nodes");
      return;
    }
    await sleep(1000);
  }
  const fa = await clientA.getFinality(tip);
  throw new Error(
    `finality did not complete (A state=${fa.state} pow=${fa.pow_work_met} finalized=${fa.finalized})`,
  );
}

const cmd = process.argv[2] || "tx";
if (cmd === "tx") {
  await smokeTx();
} else if (cmd === "finality") {
  await smokeFinality();
} else {
  throw new Error(`unknown smoke command: ${cmd}`);
}
