#!/usr/bin/env node
/**
 * Two-node tx gossip smoke:
 *   sendTransfer on node-A → poll agora_getTransaction / agora_getMempool on node-B
 *   until status is "pending".
 *
 * Usage (from repo root, with nodes already peered):
 *   node --experimental-strip-types scripts/smoke_tx.mjs
 *
 * Env:
 *   AGORA_RPC_A / AGORA_RPC_B   (default http://127.0.0.1:8545/rpc and :8546/rpc)
 *   AGORA_SMOKE_TIMEOUT_SECS    (default 60)
 *   AGORA_SMOKE_AMOUNT / AGORA_SMOKE_FEE
 */
// Import leaf modules with `.ts` extensions (Node ESM + strip-types).
import { createLightClient } from "../apps/shared/light-client/rpc.ts";
import {
  deriveAccount,
  sendTransfer,
} from "../apps/shared/light-client/wallet.ts";

const PHRASE =
  "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

const rpcA =
  process.env.AGORA_RPC_A || "http://127.0.0.1:8545/rpc";
const rpcB =
  process.env.AGORA_RPC_B || "http://127.0.0.1:8546/rpc";
const timeoutSecs = Number(process.env.AGORA_SMOKE_TIMEOUT_SECS || 60);
const amount = Number(process.env.AGORA_SMOKE_AMOUNT || 1);
const fee = Number(process.env.AGORA_SMOKE_FEE || 1);

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function main() {
  const from = deriveAccount(PHRASE, 0);
  const to = deriveAccount(PHRASE, 1);
  console.log(`from  ${from.addressHex}`);
  console.log(`to    ${to.addressHex}`);
  console.log(`rpc A ${rpcA}`);
  console.log(`rpc B ${rpcB}`);

  const clientA = createLightClient({ rpcUrl: rpcA });
  const clientB = createLightClient({ rpcUrl: rpcB });

  const { balance } = await clientA.getBalance(from.addressHex);
  console.log(`balance on A: ${balance}`);
  if (balance < amount + fee) {
    throw new Error(
      `insufficient premine on A (have ${balance}, need ${amount + fee}). Wipe both nodes and restart with the same AGORA_PREMINE_ADDRESS.`,
    );
  }

  const { tx_id } = await sendTransfer(clientA, {
    mnemonic: PHRASE,
    accountIndex: 0,
    toAddressHex: to.addressHex,
    amount,
    fee,
  });
  console.log(`submitted on A: ${tx_id}`);

  const onA = await clientA.getTransaction(tx_id);
  if (onA.status !== "pending") {
    throw new Error(`expected pending on A, got ${onA.status}`);
  }

  const deadline = Date.now() + timeoutSecs * 1000;
  while (Date.now() < deadline) {
    const lookup = await clientB.getTransaction(tx_id);
    let poolNote = "";
    let inPool = false;
    try {
      const pool = await clientB.getMempool(128);
      inPool = pool.transactions.some((t) => t.tx_id === tx_id);
      poolNote = ` mempool_count=${pool.count} in_pool=${inPool}`;
    } catch {
      poolNote = " mempool=n/a";
    }
    console.log(`B status=${lookup.status}${poolNote}`);
    if (lookup.status === "pending" || inPool) {
      console.log("tx gossip smoke OK — B sees pending transfer from A");
      return;
    }
    if (lookup.status === "confirmed") {
      console.log("tx gossip smoke OK — B already confirmed (mined during wait)");
      return;
    }
    await sleep(1000);
  }
  throw new Error(
    `timed out after ${timeoutSecs}s waiting for tx ${tx_id} on B`,
  );
}

main().catch((err) => {
  console.error(`error: ${err instanceof Error ? err.message : err}`);
  process.exit(1);
});
