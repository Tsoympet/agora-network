import { useEffect, useMemo, useState } from "react";
import { StatusBar } from "expo-status-bar";
import {
  Image,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { agoraBrand } from "../shared/brand/tokens";
import {
  createLightClient,
  shortHash,
  startTipSync,
  type TipSyncSnapshot,
} from "../shared/light-client";

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

  useEffect(() => startTipSync({ client, pollMs: POLL_MS, onUpdate: setSnap }), [
    client,
  ]);

  const statusColor =
    snap.status === "ok"
      ? agoraBrand.colors.cyan
      : snap.status === "error"
        ? "#d65a5a"
        : agoraBrand.colors.inkMuted;

  return (
    <View style={styles.shell}>
      <StatusBar style="light" />
      <ScrollView contentContainerStyle={styles.content}>
        <Image source={require("./assets/icon.png")} style={styles.icon} />
        <Text style={styles.brand}>Agora Network</Text>
        <Text style={styles.lede}>
          Light client shell. Tip sync follows the live DAG over HTTP JSON-RPC.
        </Text>

        <Text style={styles.eyebrow}>Light client</Text>
        <Text style={styles.meta}>
          <Text style={{ color: statusColor }}>
            {snap.status === "ok"
              ? "synced"
              : snap.status === "error"
                ? "offline"
                : "connecting"}
          </Text>
          {` · ${snap.tips.length} tip${snap.tips.length === 1 ? "" : "s"}`}
        </Text>
        <Text style={styles.rpc}>{client.rpcUrl}</Text>
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
  },
  footer: {
    marginTop: 20,
    color: agoraBrand.colors.inkMuted,
    fontSize: 12,
  },
});
