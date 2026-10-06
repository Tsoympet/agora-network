import { useEffect, useState } from "react";
import { Pressable, StyleSheet, Text, TextInput, View } from "react-native";
import { agoraBrand } from "../shared/brand/tokens";
import {
  compareHeaderSpines,
  shortHash,
  verifyDrcObjectHeaderProof,
  verifyIncomingTlt,
  verifyReportedFinality,
  verifySelectedParentSpine,
  type LightClient,
  type LightHeaderRecord,
} from "../shared/light-client";

export function LightClientPanel({
  client,
  network,
  genesisHash,
}: {
  client: LightClient;
  network: string | null;
  genesisHash: string | null;
}) {
  const [spine, setSpine] = useState<LightHeaderRecord[]>([]);
  const [spineNote, setSpineNote] = useState("Waiting for a canonical header spine.");
  const [finalityNote, setFinalityNote] = useState("Finality not checked.");
  const [txId, setTxId] = useState("");
  const [txNote, setTxNote] = useState<string | null>(null);
  const [objectId, setObjectId] = useState("");
  const [objectNote, setObjectNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!genesisHash) return;
    let cancelled = false;
    let previous: LightHeaderRecord[] = [];
    async function poll() {
      try {
        const chain = await client.getLightHeaders({ limit: 64 });
        if (cancelled) return;
        if (network && chain.network && chain.network !== network) {
          setSpineNote(`Wrong network: node spine says ${chain.network}.`);
          setSpine([]);
          return;
        }
        if (!chain.reaches_genesis) throw new Error("header window does not reach genesis");
        verifySelectedParentSpine(chain.headers, genesisHash!);
        const relation = compareHeaderSpines(previous, chain.headers);
        previous = chain.headers;
        setSpine(chain.headers);
        setSpineNote(
          relation === "reorg"
            ? `Reorg detected. Verified ${chain.headers.length} selected-parent header${chain.headers.length === 1 ? "" : "s"} to genesis ${shortHash(genesisHash!)}.`
            : `Verified ${chain.headers.length} selected-parent header${chain.headers.length === 1 ? "" : "s"} to genesis ${shortHash(genesisHash!)}. RandomX stays on the full node.`,
        );
        try {
          const state = verifyReportedFinality(chain.finality, chain.tip);
          if (chain.finality.node_state && chain.finality.node_state !== state) {
            throw new Error(`node finality ${chain.finality.node_state} does not match stake totals`);
          }
          setFinalityNote(`${state}. Quorum totals are not validator signatures.`);
        } catch (err) {
          setFinalityNote(err instanceof Error ? err.message : "finality unverified");
        }
      } catch (err) {
        if (cancelled) return;
        setSpine([]);
        setSpineNote(err instanceof Error ? err.message : "header sync failed");
        setFinalityNote("Finality not checked.");
      }
    }
    void poll();
    const id = setInterval(() => void poll(), 8000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [client, genesisHash, network]);

  async function onVerifyTx() {
    setBusy(true);
    setTxNote(null);
    try {
      const response = await client.getTltInclusionProof(txId.trim());
      const verified = verifyIncomingTlt(response, spine);
      setTxNote(`Included in ${shortHash(verified.headerHash)}. Merkle proof checked on this phone.`);
    } catch (err) {
      setTxNote(err instanceof Error ? err.message : "verification failed");
    } finally {
      setBusy(false);
    }
  }

  async function onQueryObject() {
    setBusy(true);
    setObjectNote(null);
    try {
      const result = await client.getDrcObject(objectId.trim());
      try {
        verifyDrcObjectHeaderProof(result.object);
      } catch (err) {
        const reason = err instanceof Error ? err.message : "not header-proven";
        if (result.status === "unknown" || !result.object) {
          setObjectNote(`Unknown object. ${reason}`);
        } else {
          const kind =
            result.object && typeof result.object === "object" && "kind" in result.object
              ? String(result.object.kind)
              : "object";
          setObjectNote(`Node-reported ${kind}. ${reason}`);
        }
      }
    } catch (err) {
      setObjectNote(err instanceof Error ? err.message : "object query failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <View>
      <Text style={styles.eyebrow}>Light client</Text>
      <Text style={styles.meta}>
        Header links, TLT inclusion, and separate OVL/DRC quorum totals. Keys stay on this phone.
      </Text>
      <Text style={styles.meta}>{spineNote}</Text>
      <Text style={styles.finality}>{finalityNote}</Text>
      <TextInput
        value={txId}
        onChangeText={setTxId}
        placeholder="TLT tx id (64 hex)"
        placeholderTextColor={agoraBrand.colors.inkMuted}
        autoCapitalize="none"
        autoCorrect={false}
        style={styles.input}
      />
      <Pressable onPress={() => void onVerifyTx()} disabled={busy || !txId.trim()} style={styles.button}>
        <Text style={styles.buttonLabel}>Verify TLT inclusion</Text>
      </Pressable>
      {txNote ? <Text style={styles.meta}>{txNote}</Text> : null}
      <TextInput
        value={objectId}
        onChangeText={setObjectId}
        placeholder="DRC object id (64 hex)"
        placeholderTextColor={agoraBrand.colors.inkMuted}
        autoCapitalize="none"
        autoCorrect={false}
        style={styles.input}
      />
      <Pressable
        onPress={() => void onQueryObject()}
        disabled={busy || !objectId.trim()}
        style={styles.button}
      >
        <Text style={styles.buttonLabel}>Query DRC object</Text>
      </Pressable>
      {objectNote ? <Text style={styles.meta}>{objectNote}</Text> : null}
    </View>
  );
}

const styles = StyleSheet.create({
  eyebrow: {
    marginTop: 36,
    color: agoraBrand.colors.goldSoft,
    fontFamily: "Cinzel",
    fontSize: 12,
    letterSpacing: 2,
    textTransform: "uppercase",
  },
  meta: {
    marginTop: 10,
    color: agoraBrand.colors.inkMuted,
    fontFamily: "Inter",
    fontSize: 15,
  },
  finality: {
    marginTop: 8,
    color: agoraBrand.colors.cyan,
    fontFamily: "Inter",
    fontSize: 15,
  },
  input: {
    marginTop: 12,
    borderWidth: 1,
    borderColor: "rgba(197, 152, 53, 0.35)",
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 10,
    color: agoraBrand.colors.ink,
    fontFamily: "Inter",
  },
  button: {
    marginTop: 8,
    alignSelf: "flex-start",
    borderWidth: 1,
    borderColor: "rgba(6, 187, 223, 0.45)",
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 8,
  },
  buttonLabel: {
    color: agoraBrand.colors.cyan,
    fontFamily: "Inter",
    fontSize: 14,
  },
});
