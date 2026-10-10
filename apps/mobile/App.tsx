import { useEffect, useMemo, useState } from "react";
import * as Clipboard from "expo-clipboard";
import { StatusBar } from "expo-status-bar";
import {
  Image,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";
import { agoraBrand } from "../shared/brand/tokens";
import * as SecureStore from "expo-secure-store";
import {
  addressBech32FromMnemonic,
  clearPersistedVault,
  createLightClient,
  generateMnemonic,
  keyValueVault,
  loadSealedVault,
  networkAccent,
  networkHrpHint,
  networkLabel,
  openVault,
  parseAddress,
  persistSealedVault,
  sealVault,
  sendAccountTransfer,
  sendDrcOfferCancel,
  sendDrcOfferCreate,
  sendDrcPayment,
  DRC_ACCOUNT_POLICY_ACTIONS,
  DRC_WALLET_FAMILY_LANES,
  drcFamilyNeedsAmount,
  drcFamilyNeedsExtraHex,
  drcFamilyNeedsIssued,
  drcFamilyNeedsObjectId,
  drcFamilyNeedsPolicyAction,
  drcFamilyNeedsRecipient,
  drcFamilyNeedsSignerList,
  sendDrcWalletFamily,
  sendOvlExecution,
  type DrcAccountPolicyAction,
  type DrcWalletFamilyLane,
  sendTltCovenant,
  sendTransfer,
  shortAddress,
  shortHash,
  startTipSync,
  walletNetworkFromNode,
  watchTransaction,
  type LightAccountBalances,
  type LightNodeInfo,
  type LightTxLookup,
  type LightUtxo,
  type TipSyncSnapshot,
} from "../shared/light-client";

const vaultStorage = keyValueVault(SecureStore);

function env(name: string): string | undefined {
  try {
    // Expo inlines EXPO_PUBLIC_* at bundle time.
    return (globalThis as { process?: { env?: Record<string, string> } }).process
      ?.env?.[name];
  } catch {
    return undefined;
  }
}

const RPC_URL = env("EXPO_PUBLIC_AGORA_RPC_URL") || "http://127.0.0.1:8545/rpc";
const POLL_MS = Number(env("EXPO_PUBLIC_AGORA_POLL_MS")) || 2000;

export default function App() {
  const client = useMemo(() => createLightClient({ rpcUrl: RPC_URL }), []);
  const [snap, setSnap] = useState<TipSyncSnapshot>({
    status: "idle",
    tips: [],
    tipBlocks: [],
    error: null,
    updatedAt: null,
  });
  const [nodeInfo, setNodeInfo] = useState<LightNodeInfo | null>(null);
  const [address, setAddress] = useState("");
  const [balance, setBalance] = useState<number | null>(null);
  const [accountBalances, setAccountBalances] =
    useState<LightAccountBalances | null>(null);
  const [utxos, setUtxos] = useState<LightUtxo[]>([]);
  const [walletError, setWalletError] = useState<string | null>(null);
  const [walletBusy, setWalletBusy] = useState(false);

  const [mnemonic, setMnemonic] = useState("");
  const [vaultPassword, setVaultPassword] = useState("");
  const [vaultHasBlob, setVaultHasBlob] = useState(false);
  const [vaultUnlocked, setVaultUnlocked] = useState(false);
  const [vaultBusy, setVaultBusy] = useState(false);
  const [vaultMsg, setVaultMsg] = useState<string | null>(null);
  const [toAddress, setToAddress] = useState("");
  const [amount, setAmount] = useState("1");
  const [fee, setFee] = useState("1");
  const [sendLane, setSendLane] = useState<
    | "tlt"
    | "tlt-covenant"
    | "ovl"
    | "ovl-exec"
    | "drc"
    | "drc-offer"
    | "drc-offer-cancel"
    | "drc-ticket"
    | DrcWalletFamilyLane
  >("tlt");
  const [offerIssuer, setOfferIssuer] = useState("");
  const [offerCurrency, setOfferCurrency] = useState("USD");
  const [offerGets, setOfferGets] = useState("1");
  const [offerId, setOfferId] = useState("");
  const [extraHex, setExtraHex] = useState("");
  const [settleDelay, setSettleDelay] = useState("1");
  const [quorum, setQuorum] = useState("1");
  const [signerEntries, setSignerEntries] = useState("");
  const [signerAction, setSignerAction] = useState<"set" | "delete">("set");
  const [policyAction, setPolicyAction] = useState<DrcAccountPolicyAction>(
    "set_require_destination_tag",
  );
  const [sendBusy, setSendBusy] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const [lastTxId, setLastTxId] = useState<string | null>(null);
  const [txLookup, setTxLookup] = useState<LightTxLookup | null>(null);
  const [receiveBech32, setReceiveBech32] = useState<string | null>(null);
  const [receiveHex, setReceiveHex] = useState<string | null>(null);
  const [copyHint, setCopyHint] = useState<string | null>(null);

  useEffect(() => startTipSync({ client, pollMs: POLL_MS, onUpdate: setSnap }), [
    client,
  ]);

  useEffect(() => {
    void loadSealedVault(vaultStorage).then((sealed) => {
      setVaultHasBlob(sealed !== null);
    });
  }, []);

  useEffect(() => {
    let cancelled = false;
    async function pollNode() {
      try {
        const info = await client.getNodeInfo();
        if (!cancelled) setNodeInfo(info);
      } catch {
        // Retain last successful nodeInfo / network across transient failures.
      }
    }
    void pollNode();
    const id = setInterval(pollNode, Math.max(POLL_MS, 4000));
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [client]);

  useEffect(() => {
    if (!lastTxId) {
      setTxLookup(null);
      return;
    }
    return watchTransaction({
      client,
      txId: lastTxId,
      pollMs: POLL_MS,
      onUpdate: setTxLookup,
    });
  }, [client, lastTxId]);

  const walletNetwork = nodeInfo
    ? walletNetworkFromNode(nodeInfo.network)
    : null;
  const networkReady = walletNetwork !== null;
  const netLabel = nodeInfo ? networkLabel(nodeInfo.network) : null;
  const netAccent = networkAccent(nodeInfo?.network);

  useEffect(() => {
    if (!networkReady || !mnemonic.trim()) return;
    try {
      const bech32 = addressBech32FromMnemonic(mnemonic, 0, "", walletNetwork);
      setReceiveBech32(bech32);
      setReceiveHex(parseAddress(bech32));
      setAddress(bech32);
    } catch {
      /* ignore while typing mnemonic */
    }
  }, [walletNetwork, networkReady, mnemonic]);

  const statusColor =
    snap.status === "ok"
      ? agoraBrand.colors.cyan
      : snap.status === "error"
        ? "#d65a5a"
        : agoraBrand.colors.inkMuted;

  async function onLookup() {
    let resolved: string;
    try {
      resolved = parseAddress(address, walletNetwork ?? undefined);
    } catch (err) {
      setWalletError(
        err instanceof Error
          ? err.message
          : "Enter a network-matching Bech32 or 40-character hex address",
      );
      setBalance(null);
      setUtxos([]);
      setAccountBalances(null);
      return;
    }
    setWalletBusy(true);
    setWalletError(null);
    try {
      const [bal, set, accounts] = await Promise.all([
        client.getBalance(resolved),
        client.getUtxos(resolved),
        client.getAccountBalances(resolved),
      ]);
      setBalance(bal.balance);
      setUtxos(set.utxos);
      setAccountBalances(accounts);
    } catch (err) {
      setBalance(null);
      setUtxos([]);
      setAccountBalances(null);
      setWalletError(err instanceof Error ? err.message : "lookup failed");
    } finally {
      setWalletBusy(false);
    }
  }

  function onDerive() {
    if (!walletNetwork) {
      setSendError("Waiting for node network…");
      return;
    }
    try {
      const bech32 = addressBech32FromMnemonic(mnemonic, 0, "", walletNetwork);
      const hex = parseAddress(bech32);
      setReceiveBech32(bech32);
      setReceiveHex(hex);
      setAddress(bech32);
      setSendError(null);
      setCopyHint(null);
    } catch (err) {
      setReceiveBech32(null);
      setReceiveHex(null);
      setSendError(err instanceof Error ? err.message : "invalid mnemonic");
    }
  }

  function onGenerate() {
    if (!walletNetwork) {
      setSendError("Waiting for node network…");
      return;
    }
    const phrase = generateMnemonic(128);
    setMnemonic(phrase);
    setVaultUnlocked(true);
    try {
      const bech32 = addressBech32FromMnemonic(phrase, 0, "", walletNetwork);
      const hex = parseAddress(bech32);
      setReceiveBech32(bech32);
      setReceiveHex(hex);
      setAddress(bech32);
      setSendError(null);
      setCopyHint(null);
      setVaultMsg("New mnemonic in memory — set password and Save vault");
    } catch (err) {
      setReceiveBech32(null);
      setReceiveHex(null);
      setSendError(err instanceof Error ? err.message : "generate failed");
    }
  }

  async function onSaveVault() {
    setVaultBusy(true);
    setVaultMsg(null);
    try {
      const sealed = await sealVault(mnemonic, vaultPassword);
      await persistSealedVault(vaultStorage, sealed);
      setVaultHasBlob(true);
      setVaultUnlocked(true);
      setVaultMsg("Vault saved to SecureStore");
    } catch (err) {
      setVaultMsg(err instanceof Error ? err.message : "save failed");
    } finally {
      setVaultBusy(false);
    }
  }

  async function onUnlockVault() {
    setVaultBusy(true);
    setVaultMsg(null);
    try {
      if (!walletNetwork) {
        throw new Error("Waiting for node network…");
      }
      const sealed = await loadSealedVault(vaultStorage);
      if (!sealed) throw new Error("no vault on this device");
      const phrase = await openVault(sealed, vaultPassword);
      setMnemonic(phrase);
      setVaultUnlocked(true);
      const bech32 = addressBech32FromMnemonic(phrase, 0, "", walletNetwork);
      setReceiveBech32(bech32);
      setReceiveHex(parseAddress(bech32));
      setAddress(bech32);
      setVaultMsg("Vault unlocked");
    } catch (err) {
      setVaultMsg(err instanceof Error ? err.message : "unlock failed");
    } finally {
      setVaultBusy(false);
    }
  }

  function onLockVault() {
    setMnemonic("");
    setVaultUnlocked(false);
    setVaultPassword("");
    setVaultMsg("Locked — mnemonic cleared from memory");
  }

  async function onDeleteVault() {
    setVaultBusy(true);
    try {
      await clearPersistedVault(vaultStorage);
      setVaultHasBlob(false);
      setMnemonic("");
      setVaultUnlocked(false);
      setVaultPassword("");
      setVaultMsg("Persisted vault deleted");
    } finally {
      setVaultBusy(false);
    }
  }

  async function onCopyReceive() {
    if (!receiveBech32 || !networkReady) return;
    try {
      await Clipboard.setStringAsync(receiveBech32);
      setCopyHint("Copied Bech32 address");
    } catch {
      setCopyHint("Clipboard unavailable");
    }
  }

  async function onSend() {
    if (!walletNetwork) {
      setSendError("Waiting for node network…");
      return;
    }
    const amt = Number(amount);
    const feeN = Number(fee);
    const amountOptional =
      sendLane === "drc-offer-cancel" ||
      sendLane === "drc-ticket" ||
      (DRC_WALLET_FAMILY_LANES.some((lane) => lane.id === sendLane) &&
        !drcFamilyNeedsAmount(sendLane));
    if (!amountOptional && (!Number.isFinite(amt) || amt <= 0)) {
      setSendError("Amount must be a positive number");
      return;
    }
    if (!Number.isFinite(feeN) || feeN < 1) {
      setSendError("Fee must be ≥ 1 (min relay)");
      return;
    }
    if (sendLane === "drc-offer" && (!Number.isFinite(Number(offerGets)) || Number(offerGets) <= 0)) {
      setSendError("Offer gets amount must be a positive number");
      return;
    }
    setSendBusy(true);
    setSendError(null);
    setLastTxId(null);
    setTxLookup(null);
    try {
      const common = {
        mnemonic,
        network: walletNetwork,
        fee: Math.floor(feeN),
      };
      let id: string;
      let fromBech32: string;
      let fromHex: string;
      if (sendLane === "tlt") {
        const { tx_id, built } = await sendTransfer(client, {
          ...common,
          toAddressHex: toAddress.trim(),
          amount: Math.floor(amt),
        });
        id = tx_id;
        fromBech32 = built.fromBech32;
        fromHex = built.from;
      } else if (sendLane === "tlt-covenant") {
        const { id: submitted, built } = await sendTltCovenant(client, {
          ...common,
          toAddress: toAddress.trim(),
          amount: Math.floor(amt),
        });
        id = submitted;
        fromBech32 = built.fromBech32;
        fromHex = built.from;
      } else if (sendLane === "ovl") {
        const { id: submitted, built } = await sendAccountTransfer(client, {
          ...common,
          asset: "OVL",
          toAddress: toAddress.trim(),
          amount: Math.floor(amt),
        });
        id = submitted;
        fromBech32 = built.fromBech32;
        fromHex = built.from;
      } else if (sendLane === "ovl-exec") {
        const { id: submitted, built } = await sendOvlExecution(client, {
          mnemonic,
          network: walletNetwork,
          toAddress: toAddress.trim(),
          value: Math.floor(amt),
          feePerGas: Math.floor(feeN),
        });
        id = submitted;
        fromBech32 = built.fromBech32;
        fromHex = built.from;
      } else if (sendLane === "drc") {
        const { id: submitted, built } = await sendDrcPayment(client, {
          ...common,
          toAddress: toAddress.trim(),
          amount: Math.floor(amt),
        });
        id = submitted;
        fromBech32 = built.fromBech32;
        fromHex = built.from;
      } else if (sendLane === "drc-offer") {
        const { id: submitted, built } = await sendDrcOfferCreate(client, {
          ...common,
          takerPaysAmount: Math.floor(amt),
          takerGetsAmount: Math.floor(Number(offerGets)),
          issuer: offerIssuer.trim(),
          currency: offerCurrency,
        });
        id = submitted;
        fromBech32 = built.fromBech32;
        fromHex = built.from;
      } else if (sendLane === "drc-offer-cancel") {
        const { id: submitted, built } = await sendDrcOfferCancel(client, {
          ...common,
          offerId: offerId.trim(),
        });
        id = submitted;
        fromBech32 = built.fromBech32;
        fromHex = built.from;
      } else {
        const { id: submitted, built } = await sendDrcWalletFamily(
          client,
          sendLane,
          {
            ...common,
            recipient: toAddress.trim(),
            amount: Math.floor(amt),
            objectId: offerId.trim(),
            issuer: offerIssuer.trim(),
            currency: offerCurrency,
            extraHex: extraHex.trim(),
            settleDelay: Math.floor(Number(settleDelay)),
            quorum: Math.floor(Number(quorum)),
            signerEntries,
            signerAction,
            policyAction,
          },
        );
        id = submitted;
        fromBech32 = built.fromBech32;
        fromHex = built.from;
      }
      setLastTxId(id);
      setReceiveBech32(fromBech32);
      setReceiveHex(fromHex);
      setAddress(fromBech32);
      const [bal, set, accounts] = await Promise.all([
        client.getBalance(fromHex),
        client.getUtxos(fromHex),
        client.getAccountBalances(fromHex),
      ]);
      setBalance(bal.balance);
      setUtxos(set.utxos);
      setAccountBalances(accounts);
    } catch (err) {
      setSendError(err instanceof Error ? err.message : "send failed");
    } finally {
      setSendBusy(false);
    }
  }

  return (
    <View style={styles.shell}>
      <StatusBar style="light" />
      <ScrollView contentContainerStyle={styles.content}>
        <Image source={require("./assets/icon.png")} style={styles.icon} />
        <Text style={styles.brand}>Agora Network</Text>
        <View
          style={[styles.netBadge, { borderBottomColor: netAccent }]}
          accessibilityLabel={
            netLabel
              ? `Network ${netLabel}`
              : "Waiting for node network"
          }
        >
          <View style={[styles.netDot, { backgroundColor: netAccent }]} />
          <Text style={[styles.netLabel, { color: netAccent }]}>
            {netLabel ?? "Connecting…"}
          </Text>
          {nodeInfo ? (
            <Text style={styles.netHrp}>
              {networkHrpHint(nodeInfo.network)}
            </Text>
          ) : null}
        </View>
        <Text style={styles.lede}>
          Mobile wallet: Bech32 receive, BIP-39 send, and live DAG tip sync over
          HTTP JSON-RPC.
        </Text>

        <Text style={styles.eyebrow}>Node</Text>
        <Text style={styles.meta}>
          <Text style={{ color: statusColor }}>
            {snap.status === "ok"
              ? "synced"
              : snap.status === "error"
                ? "offline"
                : "connecting"}
          </Text>
          {` · ${snap.tips.length} tip${snap.tips.length === 1 ? "" : "s"}`}
          {nodeInfo
            ? ` · mempool ${nodeInfo.mempool_count} · ${nodeInfo.pow_algorithm}`
            : ""}
          {nodeInfo
            ? nodeInfo.archival
              ? " · archival"
              : ` · hot ${nodeInfo.hot_window}`
            : ""}
          {nodeInfo?.connected_peers != null
            ? ` · peers ${nodeInfo.connected_peers}`
            : ""}
          {nodeInfo?.genesis_hash
            ? ` · genesis ${nodeInfo.genesis_hash.slice(0, 10)}…`
            : ""}
        </Text>
        <Text style={styles.rpc}>
          {client.rpcUrl}
          {nodeInfo?.version ? ` · ${nodeInfo.version}` : ""}
        </Text>
        {snap.error ? <Text style={styles.error}>{snap.error}</Text> : null}

        <View style={styles.tipList}>
          {snap.tips.slice(0, 8).map((tip) => {
            const block = snap.tipBlocks.find((b) => b.id === tip);
            return (
              <Text key={tip} style={styles.tipRow}>
                <Text style={{ color: agoraBrand.colors.cyan }}>tip </Text>
                {shortHash(tip)}
                {block
                  ? ` · bits ${block.header.bits} · parents ${block.header.parents.length}`
                  : ""}
              </Text>
            );
          })}
          {snap.status === "ok" && snap.tips.length === 0 ? (
            <Text style={styles.meta}>No tips yet</Text>
          ) : null}
        </View>

        {snap.updatedAt ? (
          <Text style={styles.footer}>
            Updated {new Date(snap.updatedAt).toLocaleTimeString()} · poll{" "}
            {POLL_MS}ms
          </Text>
        ) : null}

        <Text style={styles.eyebrow}>Receive</Text>
        <Text style={styles.meta}>
          {networkReady
            ? `Primary address is Bech32m (${networkHrpHint(walletNetwork)}). Hex is secondary.`
            : "Waiting for node network before deriving a receive address."}
        </Text>
        {receiveBech32 && networkReady ? (
          <>
            <Text style={[styles.tipRow, { color: agoraBrand.colors.cyan }]}>
              {receiveBech32}
            </Text>
            {receiveHex ? (
              <Text style={styles.tipRow}>hex {receiveHex}</Text>
            ) : null}
            <Pressable
              onPress={() => void onCopyReceive()}
              disabled={!networkReady}
              style={styles.lookupBtn}
            >
              <Text style={styles.lookupLabel}>Copy address</Text>
            </Pressable>
            {copyHint ? <Text style={styles.meta}>{copyHint}</Text> : null}
          </>
        ) : (
          <Text style={styles.meta}>
            {networkReady
              ? "Generate or derive a mnemonic below to get a receive address."
              : "Connect to a node to unlock wallet actions."}
          </Text>
        )}

        <Text style={styles.eyebrow}>Wallet</Text>
        <View style={styles.walletRow}>
          <TextInput
            value={address}
            onChangeText={setAddress}
            placeholder={
              networkReady
                ? `${networkHrpHint(walletNetwork)} or 40-hex`
                : "40-hex (or wait for network)"
            }
            placeholderTextColor={agoraBrand.colors.inkMuted}
            autoCapitalize="none"
            autoCorrect={false}
            style={styles.input}
          />
          <Pressable
            onPress={onLookup}
            disabled={walletBusy}
            style={styles.lookupBtn}
          >
            <Text style={styles.lookupLabel}>{walletBusy ? "…" : "Lookup"}</Text>
          </Pressable>
        </View>
        {walletError ? <Text style={styles.error}>{walletError}</Text> : null}
        {balance !== null ? (
          <Text style={styles.meta}>
            TLT {balance} base units · {utxos.length} UTXO
            {utxos.length === 1 ? "" : "s"}
          </Text>
        ) : null}
        {accountBalances ? (
          <Text style={styles.meta}>
            OVL {String(accountBalances.ovl.balance)} n{accountBalances.ovl.nonce}
            {" · "}
            DRC {String(accountBalances.drc.balance)} n{accountBalances.drc.nonce}
          </Text>
        ) : null}
        <View style={styles.tipList}>
          {utxos.slice(0, 8).map((u) => (
            <Text key={`${u.tx_id}:${u.index}`} style={styles.tipRow}>
              {shortHash(u.tx_id)}:{u.index} · {u.value}
            </Text>
          ))}
        </View>

        <Text style={styles.eyebrow}>Send</Text>
        <Text style={styles.meta}>
          BIP-39 vault (AES-GCM + SecureStore) · m/44&apos;/8888&apos;/0&apos;/0/0 · fee ≥ 1
        </Text>
        <TextInput
          value={vaultPassword}
          onChangeText={setVaultPassword}
          placeholder="vault password (min 8)"
          placeholderTextColor={agoraBrand.colors.inkMuted}
          secureTextEntry
          autoCapitalize="none"
          autoCorrect={false}
          style={styles.input}
        />
        <View style={styles.actions}>
          <Pressable
            onPress={() => void onUnlockVault()}
            disabled={vaultBusy || !vaultHasBlob || !networkReady}
            style={styles.lookupBtn}
          >
            <Text style={styles.lookupLabel}>Unlock</Text>
          </Pressable>
          <Pressable
            onPress={() => void onSaveVault()}
            disabled={vaultBusy || !mnemonic}
            style={styles.lookupBtn}
          >
            <Text style={styles.lookupLabel}>Save vault</Text>
          </Pressable>
          <Pressable
            onPress={onLockVault}
            disabled={!vaultUnlocked}
            style={styles.lookupBtn}
          >
            <Text style={styles.lookupLabel}>Lock</Text>
          </Pressable>
          <Pressable
            onPress={() => void onDeleteVault()}
            disabled={vaultBusy || !vaultHasBlob}
            style={styles.lookupBtn}
          >
            <Text style={styles.lookupLabel}>Delete</Text>
          </Pressable>
        </View>
        <Text style={styles.meta}>
          {vaultMsg ||
            (vaultHasBlob
              ? vaultUnlocked
                ? "Vault unlocked"
                : "Vault found — unlock"
              : "No vault — generate then Save")}
        </Text>
        <TextInput
          value={mnemonic}
          onChangeText={(v) => {
            setMnemonic(v);
            if (v.trim()) setVaultUnlocked(true);
          }}
          placeholder={
            vaultHasBlob && !vaultUnlocked
              ? "unlock vault to load mnemonic"
              : "mnemonic"
          }
          placeholderTextColor={agoraBrand.colors.inkMuted}
          autoCapitalize="none"
          autoCorrect={false}
          multiline
          style={[styles.input, styles.mnemonic]}
        />
        <View style={styles.actions}>
          <Pressable
            onPress={onGenerate}
            disabled={!networkReady}
            style={styles.lookupBtn}
          >
            <Text style={styles.lookupLabel}>Generate mnemonic</Text>
          </Pressable>
          <Pressable
            onPress={onDerive}
            disabled={!networkReady}
            style={styles.lookupBtn}
          >
            <Text style={styles.lookupLabel}>Derive address</Text>
          </Pressable>
        </View>
        {receiveBech32 && networkReady ? (
          <Text style={styles.tipRow} numberOfLines={1}>
            {shortAddress(receiveBech32)}
          </Text>
        ) : null}
        <View style={styles.actions}>
          {(
            [
              ["tlt", "TLT"],
              ["tlt-covenant", "Covenant"],
              ["ovl", "OVL"],
              ["ovl-exec", "OVL exec"],
              ["drc", "DRC"],
              ["drc-offer", "Offer"],
              ["drc-offer-cancel", "Cancel"],
              ["drc-ticket", "Ticket"],
              ...DRC_WALLET_FAMILY_LANES.filter((lane) => lane.id !== "drc-ticket").map(
                (lane) => [lane.id, lane.label.replace("DRC ", "")] as const,
              ),
            ] as const
          ).map(([id, label]) => (
            <Pressable
              key={id}
              onPress={() => setSendLane(id)}
              style={styles.lookupBtn}
            >
              <Text style={styles.lookupLabel}>
                {sendLane === id ? `· ${label}` : label}
              </Text>
            </Pressable>
          ))}
        </View>
        {sendLane === "drc-offer-cancel" || drcFamilyNeedsObjectId(sendLane) ? (
          <TextInput
            value={offerId}
            onChangeText={setOfferId}
            placeholder="object or offer id (64-hex)"
            placeholderTextColor={agoraBrand.colors.inkMuted}
            autoCapitalize="none"
            autoCorrect={false}
            style={styles.input}
          />
        ) : drcFamilyNeedsRecipient(sendLane) ||
          !DRC_WALLET_FAMILY_LANES.some((lane) => lane.id === sendLane) ? (
          <TextInput
            value={toAddress}
            onChangeText={setToAddress}
            placeholder={
              networkReady
                ? `to ${networkHrpHint(walletNetwork)} or 40-hex`
                : "waiting for node network…"
            }
            placeholderTextColor={agoraBrand.colors.inkMuted}
            autoCapitalize="none"
            autoCorrect={false}
            editable={networkReady}
            style={styles.input}
          />
        ) : null}
        {drcFamilyNeedsExtraHex(sendLane) ? (
          <>
            <TextInput
              value={extraHex}
              onChangeText={setExtraHex}
              placeholder={
                sendLane === "drc-channel-create"
                  ? "claim public key hex"
                  : "claim signature hex"
              }
              placeholderTextColor={agoraBrand.colors.inkMuted}
              autoCapitalize="none"
              autoCorrect={false}
              style={styles.input}
            />
            {sendLane === "drc-channel-create" ? (
              <TextInput
                value={settleDelay}
                onChangeText={setSettleDelay}
                placeholder="settle delay"
                placeholderTextColor={agoraBrand.colors.inkMuted}
                keyboardType="numeric"
                style={styles.input}
              />
            ) : null}
          </>
        ) : null}
        {drcFamilyNeedsSignerList(sendLane) ? (
          <>
            <View style={styles.actions}>
              {(["set", "delete"] as const).map((action) => (
                <Pressable
                  key={action}
                  onPress={() => setSignerAction(action)}
                  style={styles.lookupBtn}
                >
                  <Text style={styles.lookupLabel}>
                    {signerAction === action ? `· ${action}` : action}
                  </Text>
                </Pressable>
              ))}
            </View>
            {signerAction === "set" ? (
              <>
                <TextInput
                  value={quorum}
                  onChangeText={setQuorum}
                  placeholder="quorum"
                  placeholderTextColor={agoraBrand.colors.inkMuted}
                  keyboardType="numeric"
                  style={styles.input}
                />
                <TextInput
                  value={signerEntries}
                  onChangeText={setSignerEntries}
                  placeholder={"address,weight\none signer per line"}
                  placeholderTextColor={agoraBrand.colors.inkMuted}
                  autoCapitalize="none"
                  autoCorrect={false}
                  multiline
                  style={styles.input}
                />
              </>
            ) : null}
          </>
        ) : null}
        {drcFamilyNeedsPolicyAction(sendLane) ? (
          <View style={styles.actions}>
            {DRC_ACCOUNT_POLICY_ACTIONS.map((action) => (
              <Pressable
                key={action.id}
                onPress={() => setPolicyAction(action.id)}
                style={styles.lookupBtn}
              >
                <Text style={styles.lookupLabel}>
                  {policyAction === action.id ? `· ${action.label}` : action.label}
                </Text>
              </Pressable>
            ))}
          </View>
        ) : null}
        {sendLane === "drc-offer" || drcFamilyNeedsIssued(sendLane) ? (
          <>
            <TextInput
              value={offerIssuer}
              onChangeText={setOfferIssuer}
              placeholder="issued-asset issuer"
              placeholderTextColor={agoraBrand.colors.inkMuted}
              autoCapitalize="none"
              autoCorrect={false}
              style={styles.input}
            />
            <View style={styles.walletRow}>
              <TextInput
                value={offerCurrency}
                onChangeText={setOfferCurrency}
                placeholder="USD"
                placeholderTextColor={agoraBrand.colors.inkMuted}
                autoCapitalize="characters"
                style={styles.input}
              />
              {sendLane === "drc-offer" ? (
                <TextInput
                  value={offerGets}
                  onChangeText={setOfferGets}
                  placeholder="gets amount"
                  placeholderTextColor={agoraBrand.colors.inkMuted}
                  keyboardType="numeric"
                  style={styles.input}
                />
              ) : null}
            </View>
          </>
        ) : null}
        <View style={styles.walletRow}>
          <TextInput
            value={amount}
            onChangeText={setAmount}
            placeholder="amount"
            placeholderTextColor={agoraBrand.colors.inkMuted}
            keyboardType="numeric"
            style={styles.input}
          />
          <TextInput
            value={fee}
            onChangeText={setFee}
            placeholder="fee"
            placeholderTextColor={agoraBrand.colors.inkMuted}
            keyboardType="numeric"
            style={styles.input}
          />
        </View>
        <Pressable
          onPress={onSend}
          disabled={sendBusy || !networkReady}
          style={styles.lookupBtn}
        >
          <Text style={styles.lookupLabel}>
            {sendBusy ? "Signing…" : "Sign & send"}
          </Text>
        </Pressable>
        {sendError ? <Text style={styles.error}>{sendError}</Text> : null}
        {lastTxId ? (
          <Text style={[styles.tipRow, { color: agoraBrand.colors.cyan }]}>
            {shortHash(lastTxId)} · {txLookup?.status ?? "pending"}
            {txLookup?.status === "confirmed" && txLookup.confirmations != null
              ? ` · ${txLookup.confirmations} conf`
              : ""}
            {txLookup?.status === "confirmed" && txLookup.block_id
              ? ` @ ${shortHash(txLookup.block_id)}`
              : ""}
            {txLookup?.fee != null ? ` · fee ${txLookup.fee}` : ""}
          </Text>
        ) : null}
      </ScrollView>
    </View>
  );
}

const styles = StyleSheet.create({
  shell: {
    flex: 1,
    backgroundColor: agoraBrand.colors.obsidian,
  },
  content: {
    flexGrow: 1,
    justifyContent: "center",
    paddingHorizontal: 28,
    paddingVertical: 48,
  },
  icon: {
    width: 72,
    height: 72,
    borderRadius: 16,
  },
  brand: {
    marginTop: 18,
    color: agoraBrand.colors.gold,
    fontSize: 34,
    fontWeight: "700",
    letterSpacing: 1,
  },
  netBadge: {
    marginTop: 12,
    alignSelf: "flex-start",
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
    paddingBottom: 6,
    borderBottomWidth: 2,
  },
  netDot: {
    width: 8,
    height: 8,
    borderRadius: 4,
  },
  netLabel: {
    fontSize: 14,
    fontWeight: "700",
    letterSpacing: 2,
    textTransform: "uppercase",
  },
  netHrp: {
    marginLeft: 2,
    fontSize: 12,
    fontFamily: "monospace",
    color: agoraBrand.colors.inkMuted,
  },
  lede: {
    marginTop: 12,
    color: agoraBrand.colors.inkMuted,
    fontSize: 16,
    lineHeight: 24,
    maxWidth: 420,
  },
  eyebrow: {
    marginTop: 36,
    color: agoraBrand.colors.goldSoft,
    fontSize: 12,
    letterSpacing: 2,
    textTransform: "uppercase",
  },
  meta: {
    marginTop: 10,
    color: agoraBrand.colors.inkMuted,
    fontSize: 15,
  },
  rpc: {
    marginTop: 6,
    color: agoraBrand.colors.inkMuted,
    fontSize: 12,
    fontFamily: "monospace",
    opacity: 0.85,
  },
  error: {
    marginTop: 8,
    color: "#d65a5a",
    fontSize: 14,
  },
  tipList: {
    marginTop: 18,
    gap: 10,
  },
  tipRow: {
    color: agoraBrand.colors.ink,
    fontSize: 13,
    fontFamily: "monospace",
    marginTop: 8,
  },
  footer: {
    marginTop: 20,
    color: agoraBrand.colors.inkMuted,
    fontSize: 12,
  },
  walletRow: {
    marginTop: 12,
    flexDirection: "row",
    gap: 10,
    alignItems: "center",
  },
  actions: {
    marginTop: 4,
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 10,
  },
  input: {
    flex: 1,
    marginTop: 12,
    borderWidth: 1,
    borderColor: agoraBrand.colors.gold,
    color: agoraBrand.colors.ink,
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontFamily: "monospace",
    fontSize: 13,
  },
  mnemonic: {
    minHeight: 72,
    textAlignVertical: "top",
  },
  lookupBtn: {
    marginTop: 12,
    borderWidth: 1,
    borderColor: agoraBrand.colors.gold,
    paddingHorizontal: 14,
    paddingVertical: 10,
    alignSelf: "flex-start",
  },
  lookupLabel: {
    color: agoraBrand.colors.gold,
    fontSize: 13,
    letterSpacing: 1,
  },
});
