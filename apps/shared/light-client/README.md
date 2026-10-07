# Agora light-client helpers

Shared HTTP JSON-RPC tip sync and wallet helpers for desktop, mobile, and explorer.

```ts
import {
  addressBech32FromMnemonic,
  createLightClient,
  generateMnemonic,
  parseAddress,
  startTipSync,
} from "../shared/light-client";

const client = createLightClient({ rpcUrl: "http://127.0.0.1:8545/rpc" });
const stop = startTipSync({
  client,
  pollMs: 2000,
  onUpdate: (snap) => console.log(snap.status, snap.tips),
});

const mnemonic = generateMnemonic(128);
const bech32 = addressBech32FromMnemonic(mnemonic);
const hex = parseAddress(bech32);

const { balance } = await client.getBalance(bech32);
const info = await client.getNodeInfo();
// later: stop();
```

RPC methods: `agora_getDagTips`, `agora_getBlock`, `agora_getTransaction`,
`agora_getMempool`, `agora_getNodeInfo`, `agora_getBalance`,
`agora_getAccountBalances`, `agora_getTltCovenant`, `agora_getDrcOffer`,
`agora_getDrcAccountOffers`, `agora_getDrcBookOffers`, `agora_getDrcEscrow`,
`eth_chainId`, `eth_blockNumber`, `eth_getBalance`, `agora_getUtxos`,
`agora_submitTransaction`, `agora_submitTltCovenant`,
`agora_submitAccountTransfer`, `agora_submitOvlExecution`,
`agora_submitDrcPayment`, `agora_submitDrcOfferCreate`, remaining DRC family
submits (`agora_submitDrcEscrowCreate`, `agora_submitDrcTicketCreate`, Checks,
channels, trust lines, issued controls, tickets, keys, policy),
`agora_submitPassportAttestation`, `agora_getPassportAttestation`,
`agora_getPassportIssuerNonce`, `agora_submitHubRegistration`,
`agora_getHubRegistration`, `agora_getHubCoordinatorNonce`,
`agora_submitGrantRegistration`, `agora_getGrantRegistration`,
`agora_getGrantRegistrarNonce`, `agora_submitMissionRegistration`,
`agora_getMissionRegistration`, `agora_getMissionSponsorNonce`.

Device-local builders (`typed-lanes.ts`, `typed-lanes-drc.ts`,
`typed-lanes-passport.ts`, `typed-lanes-community.ts`) construct Agora-signed
DRC payments, native/issued offers, escrow, Checks, payment channels, tickets,
regular key, signer list, deposit preauth, account policy, trust lines, issued
controls, signed Hub-coordinator passport attestations, signed Hub / Grant /
Mission registrations, TLT P2PKH covenants, OVL account transfers, and OVL
execution v1 (empty calldata). Keys stay on the device. Raw Ethereum
envelopes are signed only by `raw-evm.ts` from an explicit secp256k1 key
(never a vault mnemonic). Agora BIP-44 addresses and Ethereum keccak
addresses are different 20-byte families.

Addresses: Bech32m HRPs `agora` / `agoratest` / `agoradev` preferred; 40-char
hex stays network-neutral. Pass the node network to `parseAddress(input, network)`
(and to `sendTransfer` / `buildSignedTransfer`) so cross-network Bech32 recipients
are rejected instead of silently re-encoded.
