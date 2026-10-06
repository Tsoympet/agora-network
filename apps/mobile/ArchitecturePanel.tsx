import { useState } from "react";
import { Pressable, Text, View } from "react-native";
import { agoraBrand } from "../shared/brand/tokens";
import { SESSION_TIMEOUT_CHOICES } from "../shared/security/desktop";
import {
  exportDeviceSeed,
  lockDeviceSpendSession,
  openDeviceSpendSession,
} from "../shared/security/session";
import {
  architecturePanelModel,
  type UxMode,
} from "../shared/settings";
import { generateMnemonic } from "../shared/wallet";

export function ArchitecturePanel(props: {
  nodeUrl: string;
  unlocked: boolean;
  onLock: () => void;
  timeoutMs: number;
  onTimeoutMs: (ms: number) => void;
}) {
  const [mode, setMode] = useState<UxMode>("beginner");
  const [note, setNote] = useState<string | null>(null);
  const model = architecturePanelModel(mode, props.nodeUrl);

  function onRefuseExport() {
    const phrase = generateMnemonic();
    const session = openDeviceSpendSession(phrase, Date.now(), props.timeoutMs);
    lockDeviceSpendSession(session);
    try {
      exportDeviceSeed(session, Date.now(), true);
      setNote("unexpected export");
    } catch (err) {
      setNote(err instanceof Error ? err.message : "locked");
    }
  }

  return (
    <View style={{ marginTop: 24 }}>
      <Text style={{ color: agoraBrand.colors.gold, letterSpacing: 2, fontSize: 12 }}>
        SETTINGS
      </Text>
      <Text style={{ color: agoraBrand.colors.ink, fontSize: 22, marginTop: 8 }}>
        Participation
      </Text>
      <Text style={{ color: agoraBrand.colors.inkMuted, marginTop: 8 }}>
        Keys stay on this device. This screen does not show a seed.
      </Text>
      <View style={{ flexDirection: "row", gap: 8, marginTop: 12 }}>
        {(["beginner", "advanced"] as const).map((item) => (
          <Pressable
            key={item}
            onPress={() => setMode(item)}
            style={{
              borderWidth: 1,
              borderColor: agoraBrand.colors.gold,
              paddingHorizontal: 12,
              paddingVertical: 8,
            }}
          >
            <Text style={{ color: agoraBrand.colors.gold }}>
              {item === "beginner" ? "Beginner" : "Advanced"}
            </Text>
          </Pressable>
        ))}
      </View>
      {[model.copy.drc, model.copy.ovl, model.copy.tlt, model.copy.community].map((line) => (
        <Text key={line} style={{ color: agoraBrand.colors.ink, marginTop: 8 }}>
          {line}
        </Text>
      ))}
      {model.surfaces.map((item) => (
        <Text key={item.id} style={{ color: agoraBrand.colors.inkMuted, marginTop: 4 }}>
          {item.label}
          {item.status === "PLANNED" ? " · PLANNED" : ""}
        </Text>
      ))}
      {model.warnings.map((warning) => (
        <Text key={warning} style={{ color: agoraBrand.colors.gold, marginTop: 6 }}>
          {warning}
        </Text>
      ))}
      <Text style={{ color: agoraBrand.colors.inkMuted, marginTop: 10 }}>
        Hardware wallet: {model.hardwareWallet}
      </Text>
      <View style={{ flexDirection: "row", flexWrap: "wrap", gap: 8, marginTop: 12 }}>
        {SESSION_TIMEOUT_CHOICES.map((ms) => (
          <Pressable
            key={ms}
            onPress={() => props.onTimeoutMs(ms)}
            style={{
              borderWidth: 1,
              borderColor: props.timeoutMs === ms ? agoraBrand.colors.cyan : agoraBrand.colors.gold,
              paddingHorizontal: 10,
              paddingVertical: 8,
            }}
          >
            <Text style={{ color: agoraBrand.colors.ink }}>{Math.round(ms / 60_000)} min</Text>
          </Pressable>
        ))}
      </View>
      <Pressable
        onPress={props.onLock}
        disabled={!props.unlocked}
        style={{ marginTop: 12, alignSelf: "flex-start" }}
      >
        <Text style={{ color: agoraBrand.colors.gold }}>Lock wallet</Text>
      </Pressable>
      <Pressable onPress={onRefuseExport} style={{ marginTop: 8, alignSelf: "flex-start" }}>
        <Text style={{ color: agoraBrand.colors.gold }}>Try seed export</Text>
      </Pressable>
      {note ? (
        <Text style={{ color: agoraBrand.colors.inkMuted, marginTop: 8 }}>{note}</Text>
      ) : null}
    </View>
  );
}
