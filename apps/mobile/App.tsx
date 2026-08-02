import { StatusBar } from "expo-status-bar";
import { Image, StyleSheet, Text, View } from "react-native";
import { agoraBrand } from "../shared/brand/tokens";

export default function App() {
  return (
    <View style={styles.shell}>
      <StatusBar style="light" />
      <Image source={require("./assets/icon.png")} style={styles.icon} />
      <Text style={styles.brand}>Agora Network</Text>
      <Text style={styles.lede}>
        Light client shell. Nexus icon and Obsidian & Gold tokens are configured
        for Expo packaging.
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  shell: {
    flex: 1,
    backgroundColor: agoraBrand.colors.obsidian,
    justifyContent: "center",
    paddingHorizontal: 28,
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
});
