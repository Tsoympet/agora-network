import { hexToBytes } from "./borshWire";
import {
  buildAccountTransfer,
  buildDrcPayment,
  requireSigner,
} from "./envelopes";
import type { LightClient, LightUtxo } from "./rpc";
import { submitBuiltEnvelope } from "./submitEnvelope";
import { chainIdForNetwork } from "./wallet";

export async function fetchTltHistory(
  client: LightClient,
  utxos: LightUtxo[],
  extraIds: string[],
): Promise<Array<{ tx_id: string; status: string }>> {
  const ids = new Set<string>();
  for (const utxo of utxos) ids.add(utxo.tx_id.toLowerCase());
  for (const id of extraIds) {
    const clean = id.trim().toLowerCase();
    if (clean.length === 64) ids.add(clean);
  }
  const out: Array<{ tx_id: string; status: string }> = [];
  for (const tx_id of [...ids].slice(0, 24)) {
    try {
      const row = await client.getTransaction(tx_id);
      out.push({ tx_id, status: row.status });
    } catch {
      out.push({ tx_id, status: "unknown" });
    }
  }
  return out;
}

export async function submitOvlTransfer(args: {
  client: LightClient;
  mnemonic: string;
  network: string;
  genesisHex: string;
  chainId: string;
  toHex: string;
  amount: bigint;
  fee: bigint;
  nonce: bigint;
}): Promise<string> {
  const account = requireSigner(args.mnemonic, args.network);
  const envelope = await buildAccountTransfer({
    account,
    asset: "OVL",
    to: hexToBytes(args.toHex),
    amount: args.amount,
    fee: args.fee,
    nonce: args.nonce,
    chainId: args.chainId || chainIdForNetwork(args.network),
    genesis: hexToBytes(args.genesisHex),
  });
  const result = (await submitBuiltEnvelope(args.client, envelope)) as {
    tx_id?: string;
    operation_id?: string;
  };
  return result.tx_id ?? result.operation_id ?? "submitted";
}

export async function submitDrcNativePayment(args: {
  client: LightClient;
  mnemonic: string;
  network: string;
  genesisHex: string;
  chainId: string;
  toHex: string;
  amount: bigint;
  fee: bigint;
  nonce: bigint;
  destinationTag?: number | null;
  sourceTag?: number | null;
  invoiceHex?: string;
  expiryBlue?: bigint | null;
}): Promise<string> {
  const account = requireSigner(args.mnemonic, args.network);
  const envelope = await buildDrcPayment({
    account,
    to: hexToBytes(args.toHex),
    amount: args.amount,
    fee: args.fee,
    destinationTag: args.destinationTag ?? null,
    sourceTag: args.sourceTag ?? null,
    invoice: hexToBytes(args.invoiceHex ?? "00".repeat(32)),
    nonce: args.nonce,
    expiry: args.expiryBlue ?? null,
    chainId: args.chainId || chainIdForNetwork(args.network),
    genesis: hexToBytes(args.genesisHex),
    network: args.network,
  });
  const result = (await submitBuiltEnvelope(args.client, envelope)) as {
    payment_id?: string;
    tx_id?: string;
  };
  return result.payment_id ?? result.tx_id ?? "submitted";
}
