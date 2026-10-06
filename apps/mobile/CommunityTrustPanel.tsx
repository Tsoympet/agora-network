import { useEffect, useMemo, useState } from "react";
import { Pressable, StyleSheet, Text, TextInput, View } from "react-native";
import { agoraBrand } from "../shared/brand/tokens";
import {
  communityTrustView,
  defaultTreasuryInputs,
  type CommunityProposalInput,
  type EcosystemCounts,
  type HubInput,
  type TreasuryInputs,
} from "../shared/community-trust";

type CommunityReader = {
  hubs: () => Promise<{ data: HubInput[] | null; label: string }>;
  proposals: () => Promise<{ data: CommunityProposalInput[] | null; label: string }>;
  ecosystem: () => Promise<{ data: EcosystemCounts | null; label: string }>;
};

export function CommunityTrustPanel({
  subjectAddress,
  community,
}: {
  subjectAddress: string | null;
  community?: CommunityReader | null;
}) {
  const [query, setQuery] = useState("");
  const [language, setLanguage] = useState("en");
  const [interests, setInterests] = useState("");
  const [treasury, setTreasury] = useState<TreasuryInputs>(defaultTreasuryInputs());
  const [fundingNote, setFundingNote] = useState<string | null>(null);
  const [hubs, setHubs] = useState<HubInput[]>([]);
  const [serviceProposals, setServiceProposals] = useState<CommunityProposalInput[]>([]);
  const [ecosystem, setEcosystem] = useState<EcosystemCounts | null>(null);
  const [serviceLabel, setServiceLabel] = useState<string | null>(null);
  useEffect(() => {
    if (!community) return;
    let cancelled = false;
    void Promise.all([community.hubs(), community.proposals(), community.ecosystem()]).then(
      ([hubView, proposalView, ecosystemView]) => {
        if (cancelled) return;
        setHubs(hubView.data ?? []);
        setServiceProposals(proposalView.data ?? []);
        setEcosystem(ecosystemView.data);
        setServiceLabel(hubView.label);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [community]);
  const view = useMemo(
    () =>
      communityTrustView({
        query,
        treasury,
        subjectAddress,
        language,
        interestFilters: interests
          .split(",")
          .map((part) => part.trim())
          .filter(Boolean),
        hubs,
        proposals: serviceProposals,
        ecosystem,
      }),
    [ecosystem, hubs, interests, language, query, serviceProposals, subjectAddress, treasury],
  );

  function setTreasuryField(key: keyof TreasuryInputs, value: string) {
    setTreasury((current) => ({ ...current, [key]: value }));
  }

  return (
    <View>
      <Text style={styles.eyebrow}>Community trust</Text>
      <Text style={styles.meta}>Scaffold. Keys stay on this device. Seed records are local data.</Text>
      <Text style={styles.body}>{view.notice}</Text>
      {serviceLabel ? <Text style={styles.meta}>{serviceLabel}</Text> : null}
      <TextInput
        accessibilityLabel="Region search"
        value={query}
        onChangeText={setQuery}
        placeholder="Greece, Europe, Asia, Americas, Athens"
        placeholderTextColor={agoraBrand.colors.inkMuted}
        style={styles.input}
        autoCorrect={false}
      />
      {view.communities.map((community) => (
        <View key={community.id} style={styles.card}>
          <Text style={styles.cardTitle}>{community.name}</Text>
          <Text style={styles.meta}>
            {[community.country, community.region, community.city].filter(Boolean).join(" · ") || "Attribute community"}
          </Text>
          <Text style={styles.body}>
            {community.languages.join(", ")} · {community.interests.join(", ")} · {community.specializations.join(", ")}
          </Text>
        </View>
      ))}

      <Text style={styles.eyebrow}>Proposals</Text>
      <Text style={styles.meta}>Scaffold cards. ON-CHAIN is a consensus record. ADVISORY is a public note.</Text>
      {view.proposals.map((proposal) => (
        <View key={proposal.id} style={styles.card}>
          <Text style={proposal.badge === "ON-CHAIN" ? styles.badgeOn : styles.badgeAdvisory}>{proposal.badge}</Text>
          <Text style={styles.cardTitle}>{proposal.id}</Text>
          <Text style={styles.body}>Creator: {proposal.creator}</Text>
          <Text style={styles.body}>Why: {proposal.why}</Text>
          <Text style={styles.body}>What changes: {proposal.what_changes}</Text>
          <Text style={styles.body}>Expected cost: {proposal.expected_cost}</Text>
          <Text style={styles.body}>Treasury impact: {proposal.treasury_impact}</Text>
          <Text style={styles.body}>
            Voting period: slots {proposal.voting_period.start_slot}–{proposal.voting_period.end_slot}
          </Text>
          <Text style={styles.body}>Eligibility: {proposal.eligibility}</Text>
          <Text style={styles.body}>
            Current votes: yes {proposal.current_votes.yes}, no {proposal.current_votes.no}, abstain{" "}
            {proposal.current_votes.abstain}, veto {proposal.current_votes.no_with_veto}
          </Text>
          <Text style={styles.body}>Final result: {proposal.final_result}</Text>
          <Text style={styles.body}>Implementation: {proposal.implementation_status}</Text>
          <Text style={styles.body}>Commits: {proposal.commit_links.join(", ") || "none yet"}</Text>
          <Text style={styles.body}>Releases: {proposal.release_links.join(", ") || "none yet"}</Text>
        </View>
      ))}
      {view.serviceProposals.map((proposal) => (
        <View key={proposal.id} style={styles.card}>
          <Text style={proposal.badge === "ON-CHAIN" ? styles.badgeOn : styles.badgeAdvisory}>{proposal.badge}</Text>
          <Text style={styles.cardTitle}>{proposal.id}</Text>
          <Text style={styles.body}>{proposal.note}</Text>
          {proposal.lines.map((line) => (
            <Text key={line.label} style={styles.body}>
              {line.label}: {line.value}
            </Text>
          ))}
        </View>
      ))}

      <Text style={styles.eyebrow}>Rewards</Text>
      <Text style={styles.meta}>TLT emission stays blocked. These controls do not submit a transaction.</Text>
      {(
        [
          ["drc_available", "DRC treasury"],
          ["drc_amount", "DRC amount"],
          ["ovl_available", "OVL treasury"],
          ["ovl_amount", "OVL amount"],
          ["tlt_available", "TLT treasury"],
          ["tlt_amount", "TLT amount"],
        ] as const
      ).map(([key, label]) => (
        <TextInput
          key={key}
          accessibilityLabel={label}
          value={treasury[key]}
          onChangeText={(value) => setTreasuryField(key, value)}
          style={styles.input}
        />
      ))}
      {view.rewards.map((row) => (
        <View key={row.label} style={styles.card}>
          <Text style={styles.body}>
            {row.label} · {row.ui_blocked ? "blocked" : "funding path"} · supply delta {row.supply_delta_base_units}
          </Text>
          <Pressable
            disabled={row.control.disabled}
            onPress={() => setFundingNote(row.funding_note)}
            style={[styles.button, row.control.disabled ? styles.buttonOff : null]}
          >
            <Text style={styles.buttonLabel}>{row.control.label}</Text>
          </Pressable>
          {row.block_reason ? <Text style={styles.meta}>{row.block_reason}</Text> : null}
        </View>
      ))}
      {fundingNote ? <Text style={styles.body}>{fundingNote}</Text> : null}

      <Text style={styles.eyebrow}>Public analytics</Text>
      <Text style={styles.meta}>{view.analytics.assumption}</Text>
      {view.analyticsRows.map((entry) => (
        <Text key={entry.key} style={styles.body}>
          {view.analyticsLabels[entry.key]}: {entry.counted ? entry.value : "not counted"}
        </Text>
      ))}

      <Text style={styles.eyebrow}>Portable identity</Text>
      <TextInput accessibilityLabel="Language preference" value={language} onChangeText={setLanguage} style={styles.input} />
      <TextInput
        accessibilityLabel="Interest filters"
        value={interests}
        onChangeText={setInterests}
        placeholder="governance, payments"
        placeholderTextColor={agoraBrand.colors.inkMuted}
        style={styles.input}
      />
      <Text style={styles.body}>
        Public subject: {subjectAddress ?? "none selected"}. Local preferences are included in this JSON. Private keys are omitted.
      </Text>
      {"error" in view.identity ? (
        <Text style={styles.error}>{view.identity.error}</Text>
      ) : (
        <Text style={styles.json}>{view.identity.json}</Text>
      )}

      <Text style={styles.eyebrow}>Trust</Text>
      {view.assumptions.map((row) => (
        <View key={row.id} style={styles.card}>
          <Text style={styles.cardTitle}>
            {row.title} · {row.posture}
          </Text>
          <Text style={styles.body}>{row.checks}</Text>
          <Text style={styles.assumption}>{row.assumption}</Text>
        </View>
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  eyebrow: {
    marginTop: 28,
    color: agoraBrand.colors.goldSoft,
    fontFamily: "Cinzel",
    fontSize: 12,
    letterSpacing: 2,
    textTransform: "uppercase",
  },
  meta: {
    marginTop: 8,
    color: agoraBrand.colors.inkMuted,
    fontSize: 14,
  },
  body: {
    marginTop: 6,
    color: agoraBrand.colors.ink,
    fontSize: 14,
    lineHeight: 20,
  },
  input: {
    marginTop: 10,
    borderWidth: 1,
    borderColor: agoraBrand.colors.gold,
    color: agoraBrand.colors.ink,
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontFamily: "monospace",
    fontSize: 13,
  },
  card: {
    marginTop: 12,
    paddingTop: 8,
    borderTopWidth: 1,
    borderTopColor: "rgba(197, 152, 53, 0.35)",
  },
  cardTitle: {
    color: agoraBrand.colors.ink,
    fontSize: 16,
    fontWeight: "600",
  },
  badgeOn: {
    color: agoraBrand.colors.cyan,
    letterSpacing: 1,
    fontSize: 12,
  },
  badgeAdvisory: {
    color: agoraBrand.colors.gold,
    letterSpacing: 1,
    fontSize: 12,
  },
  button: {
    marginTop: 8,
    alignSelf: "flex-start",
    borderWidth: 1,
    borderColor: agoraBrand.colors.cyan,
    paddingHorizontal: 12,
    paddingVertical: 8,
  },
  buttonOff: {
    opacity: 0.45,
  },
  buttonLabel: {
    color: agoraBrand.colors.cyan,
    fontSize: 13,
  },
  json: {
    marginTop: 8,
    color: agoraBrand.colors.inkMuted,
    fontFamily: "monospace",
    fontSize: 11,
  },
  assumption: {
    marginTop: 4,
    color: agoraBrand.colors.goldSoft,
    fontSize: 13,
    lineHeight: 18,
  },
  error: {
    marginTop: 8,
    color: "#d65a5a",
    fontSize: 14,
  },
});
