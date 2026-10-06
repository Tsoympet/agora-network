import { useEffect, useMemo, useState } from "react";
import { Pressable, StyleSheet, Text, TextInput, View } from "react-native";
import { CameraView, useCameraPermissions } from "expo-camera";
import { agoraBrand } from "../../shared/brand/tokens";
import { ACADEMY_CERTIFICATE } from "../../shared/academy";
import { recordVote } from "../../shared/assembly";
import { GRANT_DISBURSEMENT } from "../../shared/grants";
import { deriveAccount, type LightClient, type LightProtocolTreasuries } from "../../shared/light-client";
import { submitInspectedDrcPayment } from "../../shared/community/nodePay";
import {
  PUSH_TRANSPORT,
  advanceDrcPay,
  broadcastDrcPay,
  createCommunityClient,
  GOVERNANCE_AREAS,
  inboxNotice,
  merchantForAddress,
  notificationBody,
  parseAgoraQr,
  preferChainTreasuries,
  proposalBadge,
  recordLessonProgress,
  searchHubs,
  signDrcPayIntent,
  signSessionChallenge,
  transitionMission,
  voteEligibility,
  type CommunityClient,
  type CommunitySession,
  type DrcPayReceipt,
  type DrcPayStep,
  type InAppNotice,
  type NotificationPrefs,
  type PublicPassport,
  type WalletMode,
} from "../../shared/community";

export type CommunitySpendContext = {
  mode: WalletMode;
  mnemonic: string | null;
  network: string | null;
  genesisHash: string | null;
  chainId: string | null;
  light: LightClient | null;
};

function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

const styles = StyleSheet.create({
  block: { marginTop: 28 },
  eyebrow: {
    color: agoraBrand.colors.goldSoft,
    fontFamily: "Cinzel",
    fontSize: 12,
    letterSpacing: 2,
    textTransform: "uppercase",
  },
  title: {
    marginTop: 8,
    color: agoraBrand.colors.ink,
    fontFamily: "Cinzel",
    fontSize: 22,
  },
  meta: { marginTop: 8, color: agoraBrand.colors.inkMuted, fontSize: 14, lineHeight: 20 },
  line: { marginTop: 10, color: agoraBrand.colors.ink, fontSize: 14, lineHeight: 20 },
  source: { color: agoraBrand.colors.cyan, fontSize: 12 },
  input: {
    marginTop: 12,
    borderWidth: 1,
    borderColor: agoraBrand.colors.gold,
    color: agoraBrand.colors.ink,
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontSize: 13,
  },
  btn: {
    marginTop: 12,
    marginRight: 8,
    borderWidth: 1,
    borderColor: agoraBrand.colors.gold,
    paddingHorizontal: 12,
    paddingVertical: 8,
    alignSelf: "flex-start",
  },
  btnLabel: { color: agoraBrand.colors.gold, fontSize: 13 },
});

function Source({ source }: { source: string }) {
  return <Text style={styles.source}> {source}</Text>;
}

export function useCommunityClient(baseUrl?: string | null): CommunityClient {
  return useMemo(() => createCommunityClient({ baseUrl: baseUrl ?? null }), [baseUrl]);
}

export function PassportCard({
  client,
  address,
  spend,
}: {
  client: CommunityClient;
  address: string | null;
  spend: CommunitySpendContext;
}) {
  const [passport, setPassport] = useState<PublicPassport | null>(null);
  const [label, setLabel] = useState("Loading passport…");
  const [email, setEmail] = useState("");
  const [note, setNote] = useState("Email stays on this device. consensus: false.");
  const [session, setSession] = useState<CommunitySession | null>(null);
  const [sessionNote, setSessionNote] = useState("Sign the challenge with the vault key. A pasted token is refused.");

  useEffect(() => {
    void client.passport(address ?? undefined).then((view) => {
      setPassport(view.data);
      setLabel(`${view.label} · confirmed: no`);
    });
  }, [client, address]);

  return (
    <View style={styles.block}>
      <Text style={styles.eyebrow}>Passport</Text>
      <Text style={styles.title}>{passport?.username ?? "No passport yet"}</Text>
      <Text style={styles.meta}>{label}</Text>
      {passport ? (
        <View>
          <Text style={styles.line}>
            Overall {passport.overall}
            <Source source={passport.source} />
          </Text>
          {Object.entries(passport.reputations).map(([category, score]) => (
            <Text key={category} style={styles.line}>
              {category} {score}
              <Source source={passport.source} />
            </Text>
          ))}
          <Text style={styles.line}>
            Badges {passport.badges.map((badge) => badge.name).join(", ") || "none"} · non-transferable
          </Text>
        </View>
      ) : (
        <Text style={styles.meta}>Public passport is empty for this address.</Text>
      )}
      <TextInput
        value={email}
        onChangeText={setEmail}
        placeholder="email, local only"
        placeholderTextColor={agoraBrand.colors.inkMuted}
        autoCapitalize="none"
        style={styles.input}
      />
      <Pressable
        style={styles.btn}
        onPress={() => setNote(`Saved locally${email ? " with email" : ""}. Not consensus.`)}
      >
        <Text style={styles.btnLabel}>Save private profile</Text>
      </Pressable>
      <Text style={styles.meta}>{note}</Text>
      <Pressable
        style={styles.btn}
        onPress={() => {
          void (async () => {
            if (spend.mode !== "signing" || !spend.mnemonic?.trim() || !spend.network) {
              setSessionNote("Unlock a spend wallet. Watch-only cannot sign, and a pasted token is refused.");
              return;
            }
            try {
              const account = deriveAccount(spend.mnemonic, 0, "", spend.network);
              const challenge = await client.requestSessionChallenge(account.addressBech32);
              const signature = await signSessionChallenge(account.secretKey, challenge);
              const opened = await client.openSession({
                challenge,
                publicKey: bytesToHex(account.publicKey),
                signature,
              });
              setSession(opened);
              const profile = await client.privateProfile(opened.token);
              setSessionNote(
                `Session ${opened.address}. storage ${profile.storage}. consensus ${profile.consensus}.`,
              );
            } catch (err) {
              setSession(null);
              setSessionNote(err instanceof Error ? err.message : "session refused");
            }
          })();
        }}
      >
        <Text style={styles.btnLabel}>Sign session with vault key</Text>
      </Pressable>
      <Text style={styles.meta}>
        {sessionNote}
        {session ? ` · ${session.scopes.join(", ")}` : ""}
      </Text>
    </View>
  );
}

export function DrcPayFlow({ client, spend }: { client: CommunityClient; spend: CommunitySpendContext }) {
  const [raw, setRaw] = useState("");
  const [step, setStep] = useState<DrcPayStep>("scan");
  const [note, setNote] = useState("Paste a QR or scan one. Inspect destination and amount before signing.");
  const [fee, setFee] = useState("1");
  const [nonce, setNonce] = useState("0");
  const [receipt, setReceipt] = useState<DrcPayReceipt | null>(null);
  const [scanning, setScanning] = useState(false);
  const [permission, requestPermission] = useCameraPermissions();
  const mode = spend.mode;
  const parsed = parseAgoraQr(raw);
  const preview = parsed.ok ? parsed.preview : null;

  return (
    <View style={styles.block}>
      <Text style={styles.eyebrow}>DRC PAY</Text>
      <Text style={styles.title}>Pay with DRC</Text>
      <Text style={styles.meta}>Step {step}. Destination and amount show before you sign.</Text>
      <TextInput
        value={raw}
        onChangeText={setRaw}
        placeholder="agora:1:merchant-pay?to=…&amount=…"
        placeholderTextColor={agoraBrand.colors.inkMuted}
        autoCapitalize="none"
        multiline
        style={styles.input}
      />
      <Pressable
        style={styles.btn}
        onPress={() => {
          if (!permission?.granted) {
            void requestPermission().then((result) => {
              if (!result.granted) {
                setNote("Camera permission denied. Paste the QR instead.");
                setScanning(false);
                return;
              }
              setScanning(true);
            });
            return;
          }
          setScanning(true);
        }}
      >
        <Text style={styles.btnLabel}>Scan QR</Text>
      </Pressable>
      {scanning && permission?.granted ? (
        <CameraView
          style={{ height: 220, marginTop: 12 }}
          barcodeScannerSettings={{ barcodeTypes: ["qr"] }}
          onBarcodeScanned={({ data }) => {
            setScanning(false);
            setRaw(data);
            setStep("scan");
            setNote("Camera read a code. Inspect destination and amount before signing.");
          }}
        />
      ) : null}
      <Pressable
        style={styles.btn}
        onPress={() => {
          if (!parsed.ok) {
            setNote(parsed.error);
            setStep("scan");
            return;
          }
          if (parsed.preview.asset !== "DRC") {
            setNote("This QR is not a DRC payment.");
            return;
          }
          setStep("inspect");
          setNote(parsed.preview.summary);
        }}
      >
        <Text style={styles.btnLabel}>Inspect</Text>
      </Pressable>
      {preview?.destination && preview.amount ? (
        <View>
          <Text style={styles.line}>
            To {preview.destination}
            <Source source="community submitted" />
          </Text>
          <Text style={styles.line}>
            Amount {preview.amount} DRC
            <Source source="community submitted" />
          </Text>
          <Text style={styles.line}>Wallet mode {mode}. {mode === "watch-only" ? "Watch-only cannot spend." : "The vault key signs."}</Text>
          <TextInput value={fee} onChangeText={setFee} placeholder="fee" style={styles.input} />
          <TextInput value={nonce} onChangeText={setNonce} placeholder="nonce" style={styles.input} />
          {step === "inspect" ? (
            <Pressable
              style={styles.btn}
              onPress={() => {
                void client.merchants().then((view) => {
                  const merchant = merchantForAddress(view.data ?? [], preview.destination!);
                  setStep(advanceDrcPay("inspect"));
                  setNote(
                    merchant
                      ? `${merchant.displayName} is listed (${merchant.source}). No merchant key is stored.`
                      : "Merchant directory has no match. You can still confirm the address you see.",
                  );
                });
              }}
            >
              <Text style={styles.btnLabel}>Verify merchant</Text>
            </Pressable>
          ) : null}
          {step === "verify-merchant" ? (
            <Pressable
              style={styles.btn}
              onPress={() => {
                setStep(advanceDrcPay("verify-merchant"));
                setNote(`Confirm ${preview.amount} DRC to ${preview.destination}.`);
              }}
            >
              <Text style={styles.btnLabel}>Confirm amount and destination</Text>
            </Pressable>
          ) : null}
          {step === "confirm" ? (
            <Pressable
              style={styles.btn}
              onPress={() => {
                try {
                  signDrcPayIntent({
                    mode,
                    preview,
                    merchant: {
                      listed: false,
                      displayName: null,
                      source: "community submitted",
                      holdsMerchantKeys: false,
                    },
                    confirmed: true,
                  });
                  if (!/^[0-9]+$/.test(fee) || !/^[0-9]+$/.test(nonce)) {
                    setNote("Fee and nonce must be integers before signing.");
                    return;
                  }
                  setStep(advanceDrcPay("confirm"));
                  setNote(`Vault will sign ${preview.amount} DRC to ${preview.destination}, fee ${fee}, nonce ${nonce}.`);
                } catch (err) {
                  setNote(err instanceof Error ? err.message : "sign refused");
                }
              }}
            >
              <Text style={styles.btnLabel}>Sign</Text>
            </Pressable>
          ) : null}
          {step === "sign" ? (
            <Pressable
              style={styles.btn}
              onPress={() => {
                void (async () => {
                  try {
                    signDrcPayIntent({
                      mode,
                      preview,
                      merchant: {
                        listed: false,
                        displayName: null,
                        source: "community submitted",
                        holdsMerchantKeys: false,
                      },
                      confirmed: true,
                    });
                    let next: DrcPayReceipt;
                    if (mode === "signing" && spend.mnemonic && spend.light && spend.network && spend.genesisHash) {
                      next = await submitInspectedDrcPayment({
                        mode,
                        preview,
                        mnemonic: spend.mnemonic,
                        network: spend.network,
                        genesisHex: spend.genesisHash,
                        chainId: spend.chainId ?? "",
                        client: spend.light,
                        fee: BigInt(fee),
                        nonce: BigInt(nonce),
                      });
                    } else {
                      next = broadcastDrcPay({ mode, preview, signed: true });
                    }
                    setReceipt(next);
                    setStep("receipt");
                    setNote(`${next.note} confirmed: ${next.confirmed ? "node receipt" : "no"}.`);
                  } catch (err) {
                    setNote(err instanceof Error ? err.message : "broadcast refused");
                  }
                })();
              }}
            >
              <Text style={styles.btnLabel}>Broadcast</Text>
            </Pressable>
          ) : null}
        </View>
      ) : null}
      <Text style={styles.meta}>{note}</Text>
      {receipt ? (
        <Text style={styles.meta}>
          Broadcast {receipt.broadcast} · confirmed {receipt.confirmed ? "yes, node receipt" : "no"}
        </Text>
      ) : null}
    </View>
  );
}

export function MobileModule({
  lane,
  client,
  address,
  notifications,
  onNotifications,
  chainTreasuries = null,
  spend,
}: {
  lane: string;
  client: CommunityClient;
  address: string | null;
  notifications: NotificationPrefs;
  onNotifications: (next: NotificationPrefs) => void;
  chainTreasuries?: LightProtocolTreasuries | null;
  spend: CommunitySpendContext;
}) {
  const [replyBody, setReplyBody] = useState("");
  const [replyNote, setReplyNote] = useState("Replies go to the infrastructure forum.");
  const [inbox, setInbox] = useState<InAppNotice[]>([]);
  const [voteNote, setVoteNote] = useState("Vote results stay PLANNED.");
  const [lines, setLines] = useState<string[]>([]);
  const [label, setLabel] = useState("Loading…");
  const [region, setRegion] = useState("");

  useEffect(() => {
    let cancelled = false;
    async function load() {
      if (lane === "COMMUNITY" || lane === "HOME") {
        const [eco, forum, hubs, developers, contributions] = await Promise.all([
          client.ecosystem(),
          client.forum(),
          client.hubs(),
          client.developers(),
          client.contributions(),
        ]);
        if (cancelled) return;
        setLabel(eco.label);
        const hubsFiltered = searchHubs(hubs.data ?? [], region ? { region, name: region } : {});
        setLines([
          `Ecosystem passports ${eco.data?.passportCount ?? 0}`,
          ...hubsFiltered.map((hub) => `${hub.name} · ${hub.region}`),
          ...(forum.data ?? []).map((post) => `${post.category}: ${post.title}`),
          ...(developers.data ?? []).map((dev) => `Developer ${dev.name}`),
          ...(contributions.data ?? []).map((row) => `${row.kind}: ${row.title}`),
        ]);
      } else if (lane === "MISSIONS") {
        const view = await client.missions();
        if (cancelled) return;
        setLabel(view.label);
        setLines((view.data ?? []).map((mission) => `${mission.state} · ${mission.title}`));
      } else if (lane === "ACADEMY") {
        const view = await client.academy();
        if (cancelled) return;
        setLabel(view.label);
        setLines([
          `Certificate ${ACADEMY_CERTIFICATE.status}. ${ACADEMY_CERTIFICATE.reason}`,
          ...(view.data?.courses ?? []).map((course) => `${course.track} · ${course.title}`),
        ]);
      } else if (lane === "GRANTS") {
        const view = await client.grants();
        if (cancelled) return;
        setLabel(view.label);
        setLines((view.data ?? []).map((grant) => `${grant.title} · disbursement ${GRANT_DISBURSEMENT} · disbursesFunds ${grant.disbursesFunds}`));
      } else if (lane === "GUILDS") {
        const view = await client.guilds();
        if (cancelled) return;
        setLabel(view.label);
        setLines((view.data ?? []).map((guild) => `${guild.kind} · ${guild.memberCount} members`));
      } else if (lane === "MERCHANTS") {
        const view = await client.merchants();
        if (cancelled) return;
        setLabel(view.label);
        setLines((view.data ?? []).map((merchant) => `${merchant.displayName} · ${merchant.region}`));
      } else if (lane === "EVENTS") {
        const view = await client.events();
        if (cancelled) return;
        setLabel(view.label);
        setLines((view.data ?? []).map((item) => `${item.title} · ${item.region}`));
      } else if (lane === "ASSEMBLY") {
        const view = await client.proposals();
        const passport = (await client.passport(address ?? undefined)).data;
        if (cancelled) return;
        setLabel(view.label);
        setLines([
          ...GOVERNANCE_AREAS.map((area) => {
            const gate = passport ? voteEligibility(area.id, passport) : { label: area.eligibility };
            return `${area.label}: ${gate.label}`;
          }),
          ...(view.data ?? []).map((proposal) => `${proposalBadge(proposal).badge} · ${proposal.title}`),
          "Vote results: PLANNED. This screen does not tally votes.",
        ]);
      } else if (lane === "TREASURY") {
        const view = await client.treasury();
        if (cancelled) return;
        const rows = preferChainTreasuries(view.data ?? [], chainTreasuries);
        setLabel(chainTreasuries ? "full node agora_getProtocolTreasuries · not a header proof" : view.label);
        setLines(rows.map((row) => `${row.asset} ${row.balance} · ${row.source}`));
      } else if (lane === "SETTINGS") {
        const sample = notificationBody("Mission", "Update ready, payout 3 DRC");
        setLabel("on device");
        setLines([
          `Mission notices ${notifications.missions ? "on" : "off"}`,
          `Push transport: ${PUSH_TRANSPORT}. In-app inbox only.`,
          `Inbox sample: ${sample.body}`,
          `Wallet ${spend.mode}.`,
        ]);
      } else if (lane === "OVL") {
        setLabel("OVL BUILD");
        setLines(["Contracts execute on a full node. This phone does not host the EVM."]);
      } else if (lane === "TLT") {
        setLabel("TLT SECURE");
        setLines(["PoW stays on miners. This phone does not mine."]);
      } else if (lane === "SWAP") {
        setLabel("unavailable");
        setLines(["No swap, DEX, or bridge is available."]);
      } else if (lane === "BOUNTIES") {
        const view = await client.bounties();
        if (cancelled) return;
        setLabel(view.label);
        setLines((view.data ?? []).map((bounty) => `${bounty.title} · ${bounty.status}`));
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [lane, client, address, notifications, region, chainTreasuries]);

  return (
    <View style={styles.block}>
      <Text style={styles.eyebrow}>{lane}</Text>
      <Text style={styles.meta}>{label} · confirmed: no</Text>
      {lane === "COMMUNITY" || lane === "HOME" ? (
        <View>
          <TextInput
            value={region}
            onChangeText={setRegion}
            placeholder="hub region"
            placeholderTextColor={agoraBrand.colors.inkMuted}
            style={styles.input}
          />
          <TextInput
            value={replyBody}
            onChangeText={setReplyBody}
            placeholder="Forum reply"
            placeholderTextColor={agoraBrand.colors.inkMuted}
            style={styles.input}
          />
          <Pressable
            style={styles.btn}
            onPress={() => {
              void client.forum().then(async (view) => {
                const postId = view.data?.[0]?.id;
                if (!postId || !address) {
                  setReplyNote(address ? "No post to reply to." : "Derive an address before replying.");
                  return;
                }
                const result = await client.replyToForum({
                  postId,
                  body: replyBody,
                  authorAddress: address,
                });
                setReplyNote(result.label);
              });
            }}
          >
            <Text style={styles.btnLabel}>Post reply</Text>
          </Pressable>
          <Text style={styles.meta}>{replyNote}</Text>
          <Pressable
            style={styles.btn}
            onPress={() => {
              void client.forum().then(async (view) => {
                const postId = view.data?.[0]?.id;
                if (!postId) {
                  setLines((current) => [...current, "No post to report."]);
                  return;
                }
                const report = await client.reportForum(postId, "community report");
                setLines((current) => [...current, report.label]);
              });
            }}
          >
            <Text style={styles.btnLabel}>Report forum post</Text>
          </Pressable>
        </View>
      ) : null}
      {lane === "MISSIONS" ? (
        <Pressable
          style={styles.btn}
          onPress={() => {
            void client.missions().then(async (view) => {
              const mission = (view.data ?? []).find((row) => row.state === "AVAILABLE");
              if (!mission) {
                setLines((current) => [...current, "No available mission."]);
                return;
              }
              const result = await client.advanceMission(mission.id, "ACCEPTED");
              setLines((current) => [...current, result.label]);
              if (!result.data?.recorded) return;
              const next = transitionMission(mission.state, "ACCEPTED");
              setLines((current) =>
                current.map((line) => (line.startsWith("AVAILABLE") ? line.replace("AVAILABLE", next) : line)),
              );
            });
          }}
        >
          <Text style={styles.btnLabel}>Accept an available mission</Text>
        </Pressable>
      ) : null}
      {lane === "ACADEMY" ? (
        <Pressable
          style={styles.btn}
          onPress={() => {
            const progress = recordLessonProgress([], "course-assets", "lesson-drc");
            setLines((current) => [
              ...current,
              `Local lesson ${progress[0]?.completedLessonIds[0]}. Certificate ${ACADEMY_CERTIFICATE.status}.`,
            ]);
          }}
        >
          <Text style={styles.btnLabel}>Save lesson on device</Text>
        </Pressable>
      ) : null}
      {lane === "SETTINGS" ? (
        <Pressable
          style={styles.btn}
          onPress={() => onNotifications({ ...notifications, missions: !notifications.missions, includeAmounts: false })}
        >
          <Text style={styles.btnLabel}>Toggle mission notices</Text>
        </Pressable>
      ) : null}
      {lane === "SETTINGS" ? (
        <Pressable
          style={styles.btn}
          onPress={() => setInbox((rows) => [...rows, inboxNotice("Mission", "Update ready, payout 3 DRC", Date.now())])}
        >
          <Text style={styles.btnLabel}>Save notice to inbox</Text>
        </Pressable>
      ) : null}
      {inbox.map((notice) => (
        <Text key={notice.id} style={styles.meta}>
          Inbox · {notice.title}: {notice.body}
        </Text>
      ))}
      {lane === "ASSEMBLY" ? (
        <Pressable
          style={styles.btn}
          onPress={() => {
            try {
              recordVote();
            } catch (err) {
              setVoteNote(err instanceof Error ? err.message : "vote refused");
            }
          }}
        >
          <Text style={styles.btnLabel}>Record vote</Text>
        </Pressable>
      ) : null}
      {lane === "ASSEMBLY" ? <Text style={styles.meta}>{voteNote}</Text> : null}
      {lines.length === 0 ? <Text style={styles.meta}>Empty. Nothing confirmed.</Text> : null}
      {lines.map((line) => (
        <Text key={line} style={styles.line}>
          {line}
        </Text>
      ))}
    </View>
  );
}
