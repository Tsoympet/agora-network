import { Linking, Pressable, StyleSheet, Text, View } from "react-native";
import { agoraBrand } from "../shared/brand/tokens";
import { COMMUNITY_DOCS, communityDocHref } from "../shared/light-client/communityDocs";

export function DocsAboutPanel() {
  return (
    <View style={styles.wrap}>
      <Text style={styles.eyebrow}>Docs / About</Text>
      <Text style={styles.title}>Repository documents</Text>
      <Text style={styles.meta}>
        Static links to the markdown notes. This list does not query a node. MY AGORA
        beginner home is PLANNED.
      </Text>
      {COMMUNITY_DOCS.map((doc) => (
        <Pressable
          key={doc.id}
          onPress={() => void Linking.openURL(communityDocHref(doc.path))}
          style={styles.card}
        >
          <Text style={styles.cardTitle}>{doc.title}</Text>
          <Text style={styles.maturity}>{doc.maturity}</Text>
          <Text style={styles.meta}>{doc.summary}</Text>
          <Text style={styles.path}>{doc.path}</Text>
        </Pressable>
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  wrap: {
    marginTop: 28,
  },
  eyebrow: {
    color: agoraBrand.colors.gold,
    fontFamily: "Cinzel",
    fontSize: 12,
    letterSpacing: 2,
    textTransform: "uppercase",
  },
  title: {
    marginTop: 8,
    color: agoraBrand.colors.gold,
    fontFamily: "Cinzel",
    fontSize: 22,
  },
  meta: {
    marginTop: 8,
    color: agoraBrand.colors.inkMuted,
    fontSize: 14,
  },
  card: {
    marginTop: 12,
    borderWidth: 1,
    borderColor: agoraBrand.colors.gold,
    paddingHorizontal: 12,
    paddingVertical: 10,
  },
  cardTitle: {
    color: agoraBrand.colors.gold,
    fontSize: 16,
  },
  maturity: {
    marginTop: 4,
    color: agoraBrand.colors.cyan,
    fontSize: 12,
  },
  path: {
    marginTop: 4,
    color: agoraBrand.colors.inkMuted,
    fontFamily: "monospace",
    fontSize: 11,
  },
});
