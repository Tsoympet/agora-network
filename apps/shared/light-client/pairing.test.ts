/**
 * Device pairing: restore identity, watch-only refusal, payload rejection, RPC persist.
 * Run: `node --experimental-strip-types --import ./register-ts-ext.mjs pairing.test.ts`
 */
import assert from "node:assert/strict";

import { HDKey } from "@scure/bip32";

import { qrMatrix } from "./qrMatrix.ts";
import {
  PAIRING_GUIDE_STEPS,
  WATCH_KIND,
  buildRestorePairing,
  buildWatchPairing,
  pairingImportBlocker,
  parsePairingPayload,
  serializePairing,
  signSpend,
} from "./pairing.ts";
import {
  RPC_ENDPOINT_STORAGE_KEY,
  classifyRpcReach,
  loadRpcEndpoint,
  rpcTrustWarning,
  saveRpcEndpoint,
  validateRpcUrl,
} from "./rpcEndpoint.ts";
import { keyValueVault } from "./vault.ts";
import {
  AGORA_ACCOUNT_PATH,
  deriveAccount,
  exportAccountXpub,
  generateMnemonic,
} from "./wallet.ts";

const genesis = "ab".repeat(32);
const phrase = generateMnemonic(128);
const network = "devnet";

const desktop = deriveAccount(phrase, 0, "", network, 0);
const desktopChange = deriveAccount(phrase, 0, "", network, 1);
const restored = parsePairingPayload(
  serializePairing(buildRestorePairing({ mnemonic: phrase, network, genesis })),
);
assert.equal(restored.kind, "restore");
if (restored.kind !== "restore") throw new Error("expected restore");
const phone = deriveAccount(restored.restore.mnemonic, 0, "", restored.restore.network, 0);
assert.equal(phone.addressHex, desktop.addressHex);
assert.equal(phone.addressBech32, desktop.addressBech32);
assert.equal(Buffer.from(phone.publicKey).toString("hex"), Buffer.from(desktop.publicKey).toString("hex"));
assert.equal(phone.secretKey.length, 32);

const watch = buildWatchPairing({
  mnemonic: phrase,
  network,
  genesis,
  rpcUrl: "http://192.168.1.20:8545/rpc",
});
const watchText = serializePairing(watch);
assert.equal(watchText.includes(phrase), false);
assert.equal(/xprv|yprv|zprv/i.test(watchText), false);
assert.equal(watch.tltReceiveHex, desktop.addressHex);
assert.equal(watch.tltChangeHex, desktopChange.addressHex);
assert.equal(watch.ovlAccount, desktop.addressHex);
assert.equal(watch.drcAccount, desktop.addressHex);
assert.equal(watch.rpcUrl, "http://192.168.1.20:8545/rpc");
assert.equal(watch.xpub.startsWith("xpub"), true);
assert.equal(exportAccountXpub(phrase), watch.xpub);

const imported = parsePairingPayload(watchText);
assert.equal(imported.kind, "watch");
if (imported.kind !== "watch") throw new Error("expected watch");
assert.equal(imported.watch.tltReceiveHex, desktop.addressHex);
assert.equal(Object.hasOwn(imported.watch, "secretKey"), false);
assert.equal(Object.hasOwn(imported.watch, "mnemonic"), false);

const neutered = HDKey.fromExtendedKey(imported.watch.xpub);
assert.equal(neutered.privateKey, null);
assert.throws(() => neutered.sign(new Uint8Array(32).fill(1)), /private/i);
assert.throws(() => neutered.privateExtendedKey, /private/i);

let buildCalled = false;
await assert.rejects(
  () =>
    signSpend(imported.watch, async () => {
      buildCalled = true;
      return { ok: true };
    }),
  /watch-only wallet cannot sign or spend/,
);
assert.equal(buildCalled, false);

let spent = false;
const signed = await signSpend({ mode: "spend", mnemonic: phrase }, async (mnemonic) => {
  spent = true;
  assert.equal(mnemonic, phrase);
  return deriveAccount(mnemonic, 0, "", network, 0).addressHex;
});
assert.equal(spent, true);
assert.equal(signed, desktop.addressHex);

const loopback = buildWatchPairing({
  mnemonic: phrase,
  network,
  genesis,
  rpcUrl: "http://127.0.0.1:8545/rpc",
});
assert.equal(loopback.rpcUrl, null);
assert.equal(serializePairing(loopback).includes("127.0.0.1"), false);

const tampered = JSON.parse(watchText) as { xpub: string; rpc_url?: string };
const account = HDKey.fromMasterSeed(
  Uint8Array.from({ length: 64 }, (_, i) => i + 1),
).derive(AGORA_ACCOUNT_PATH);
tampered.xpub = account.privateExtendedKey;
account.wipePrivateData();
assert.throws(() => parsePairingPayload(JSON.stringify(tampered)), /malformed pairing payload/);

assert.throws(() => parsePairingPayload("not-json"), /malformed pairing payload/);
assert.throws(() => parsePairingPayload("[]"), /malformed pairing payload/);
assert.throws(() => parsePairingPayload(""), /malformed pairing payload/);
assert.throws(
  () =>
    parsePairingPayload(
      JSON.stringify({
        v: 2,
        kind: WATCH_KIND,
        network,
        genesis,
      }),
    ),
  /malformed pairing payload/,
);
assert.throws(
  () =>
    parsePairingPayload(
      JSON.stringify({
        v: 1,
        kind: WATCH_KIND,
        network,
        genesis,
        account: 0,
        xpub: watch.xpub,
        tlt_receive: watch.tltReceive,
        tlt_receive_hex: watch.tltReceiveHex,
        tlt_change: watch.tltChange,
        tlt_change_hex: watch.tltChangeHex,
        ovl_account: watch.ovlAccount,
        drc_account: watch.drcAccount,
        mnemonic: phrase,
      }),
    ),
  /malformed pairing payload/,
);
try {
  parsePairingPayload(
    JSON.stringify({ v: 1, kind: WATCH_KIND, mnemonic: phrase, genesis }),
  );
  assert.fail("mnemonic smuggled into a watch payload");
} catch (err) {
  assert.equal(err instanceof Error, true);
  const message = err instanceof Error ? err.message : "";
  assert.match(message, /malformed pairing payload/);
  assert.equal(message.includes(phrase), false);
}

const hostileRpc = JSON.parse(watchText) as { rpc_url: string };
hostileRpc.rpc_url = "http://127.0.0.1:8545/rpc";
assert.throws(() => parsePairingPayload(JSON.stringify(hostileRpc)), /malformed pairing payload/);

const wrongAddress = JSON.parse(watchText) as { ovl_account: string };
wrongAddress.ovl_account = "11".repeat(20);
assert.throws(() => parsePairingPayload(JSON.stringify(wrongAddress)), /malformed pairing payload/);

assert.equal(
  pairingImportBlocker(watch, { network: "dev", genesis: `0x${genesis}` }),
  null,
);
assert.match(
  pairingImportBlocker(watch, { network: "testnet", genesis }) ?? "",
  /network/,
);
assert.match(
  pairingImportBlocker(watch, { network: "devnet", genesis: "cd".repeat(32) }) ?? "",
  /genesis/,
);
assert.match(
  pairingImportBlocker(watch, { network: null, genesis: null }) ?? "",
  /Connect/,
);

assert.ok(PAIRING_GUIDE_STEPS.length >= 5);
const matrix = qrMatrix(watchText);
assert.ok(matrix.size >= 21);
assert.equal(matrix.dark.length, matrix.size * matrix.size);
assert.ok(matrix.dark.some(Boolean));

const mem = new Map<string, string>();
const storage = keyValueVault(
  {
    getItemAsync: async (key) => mem.get(key) ?? null,
    setItemAsync: async (key, value) => {
      mem.set(key, value);
    },
    deleteItemAsync: async (key) => {
      mem.delete(key);
    },
  },
  RPC_ENDPOINT_STORAGE_KEY,
);
const saved = await saveRpcEndpoint(storage, "https://rpc.example.test/rpc");
assert.equal(saved, "https://rpc.example.test/rpc");
assert.equal(await loadRpcEndpoint(storage), "https://rpc.example.test/rpc");
assert.equal(mem.get(RPC_ENDPOINT_STORAGE_KEY), "https://rpc.example.test/rpc");
await assert.rejects(
  () => saveRpcEndpoint(storage, "ftp://rpc.example.test/rpc"),
  /http or https/,
);
assert.equal(await loadRpcEndpoint(storage), "https://rpc.example.test/rpc");
assert.throws(
  () => validateRpcUrl("https://user:secret@rpc.example.test/rpc"),
  /credential/,
);
assert.equal(classifyRpcReach("http://127.0.0.1:8545/rpc"), "loopback");
assert.equal(classifyRpcReach("http://192.168.1.20:8545/rpc"), "lan");
assert.equal(classifyRpcReach("https://rpc.example.test/rpc"), "public");
assert.match(rpcTrustWarning("http://127.0.0.1:8545/rpc"), /localhost/);
assert.match(rpcTrustWarning("http://127.0.0.1:8545/rpc"), /HTTP/);
assert.match(rpcTrustWarning("https://rpc.example.test/rpc"), /this device/);

console.log("agora light client pairing checks ok");
