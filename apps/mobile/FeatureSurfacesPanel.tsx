import { useEffect, useMemo, useState } from "react";
import { Pressable, ScrollView, StyleSheet, Text, TextInput, View } from "react-native";
import { agoraBrand } from "../shared/brand/tokens";
import {
  LIGHT_FEATURE_MATRIX,
  matrixWithRpcAvailability,
  type FeatureDomain,
} from "../shared/light-client/featureMatrix";
import {
  probeLightClientRpc,
  rpcMethodsFromMatrix,
} from "../shared/light-client/rpcProbe";
import {
  verificationShort,
  type VerificationStatus,
} from "../shared/light-client/verificationStatus";
import type { LightClient, WatchOnlyWallet } from "../shared/light-client";

const TABS: FeatureDomain[] = ["network", "tlt", "drc", "ovl"];

function badgeColor(status: VerificationStatus): string {
  if (status === "verified-locally") return agoraBrand.colors.cyan;
  if (status === "node-reported") return agoraBrand.colors.gold;
  return agoraBrand.colors.inkMuted;
}

type Props = {
  client: LightClient;
  genesisHash: string | null;
  drcHex: string | null;
  watchWallet: WatchOnlyWallet | null;
};

export function FeatureSurfacesPanel({ client, genesisHash, drcHex, watchWallet }: Props) {
  const [tab, setTab] = useState<FeatureDomain>("network");
  const [matrix, setMatrix] = useState(LIGHT_FEATURE_MATRIX);
  const [policyNote, setPolicyNote] = useState<string | null>(null);

  const rows = useMemo(() => matrix.filter((row) => row.domain === tab), [matrix, tab]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const available = await probeLightClientRpc(
        client,
        rpcMethodsFromMatrix(LIGHT_FEATURE_MATRIX),
      );
      if (!cancelled) setMatrix(matrixWithRpcAvailability(LIGHT_FEATURE_MATRIX, available));
    })();
    return () => {
      cancelled = true;
    };
  }, [client]);

  async function loadPolicy() {
    if (!drcHex) return;
    try {
      const policy = await client.getDrcAccountPolicy(drcHex);
      setPolicyNote(JSON.stringify(policy).slice(0, 800));
    } catch (err) {
      setPolicyNote(err instanceof Error ? err.message : "policy unavailable");
    }
  }

  return (
    <View style={styles.wrap}>
      <Text style={styles.eyebrow}>Capabilities</Text>
      <Text style={styles.meta}>
        Verification labels: local proof, node read, or unavailable. Same matrix as desktop.
      </Text>
      <ScrollView horizontal showsHorizontalScrollIndicator={false} style={styles.tabRow}>
        {TABS.map((key) => (
          <Pressable key={key} onPress={() => setTab(key)} style={styles.tabBtn}>
            <Text style={[styles.tabText, tab === key && styles.tabActive]}>{key.toUpperCase()}</Text>
          </Pressable>
        ))}
      </ScrollView>
      {rows.map((row) => (
        <View key={row.id} style={styles.card}>
          <View style={styles.cardHead}>
            <Text style={styles.cardTitle}>{row.title}</Text>
            <Text style={[styles.badge, { color: badgeColor(row.status) }]}>
              {verificationShort(row.status)}
            </Text>
          </View>
          <Text style={styles.cardDetail}>{row.detail}</Text>
        </View>
      ))}
      {tab === "network" ? (
        <Text style={styles.meta}>
          Genesis {genesisHash ? `${genesisHash.slice(0, 12)}…` : "pending"}
        </Text>
      ) : null}
      {tab === "drc" ? (
        <Pressable onPress={() => void loadPolicy()} style={styles.action}>
          <Text style={styles.actionText}>Load DRC policy</Text>
        </Pressable>
      ) : null}
      {policyNote ? <Text style={styles.mono}>{policyNote}</Text> : null}
      {watchWallet ? (
        <Text style={styles.meta}>Watch-only — sign paths disabled on phone.</Text>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  wrap: { marginTop: 28, maxWidth: 520 },
  eyebrow: {
    color: agoraBrand.colors.gold,
    letterSpacing: 2,
    fontSize: 12,
    textTransform: "uppercase",
  },
  meta: { marginTop: 8, color: agoraBrand.colors.inkMuted, fontSize: 13 },
  tabRow: { marginTop: 12, marginBottom: 8 },
  tabBtn: { marginRight: 12, paddingVertical: 6 },
  tabText: { color: agoraBrand.colors.gold, fontSize: 13, letterSpacing: 1 },
  tabActive: { color: agoraBrand.colors.cyan },
  card: {
    marginTop: 10,
    borderWidth: 1,
    borderColor: "rgba(197,152,53,0.25)",
    borderRadius: 8,
    padding: 10,
  },
  cardHead: { flexDirection: "row", justifyContent: "space-between", gap: 8 },
  cardTitle: { color: agoraBrand.colors.ink, fontSize: 14, flex: 1 },
  badge: { fontFamily: "monospace", fontSize: 11 },
  cardDetail: { marginTop: 6, color: agoraBrand.colors.inkMuted, fontSize: 12 },
  action: {
    marginTop: 12,
    alignSelf: "flex-start",
    borderWidth: 1,
    borderColor: agoraBrand.colors.gold,
    paddingHorizontal: 12,
    paddingVertical: 8,
  },
  actionText: { color: agoraBrand.colors.gold, fontSize: 13 },
  mono: {
    marginTop: 8,
    fontFamily: "monospace",
    fontSize: 11,
    color: agoraBrand.colors.cyan,
  },
});
