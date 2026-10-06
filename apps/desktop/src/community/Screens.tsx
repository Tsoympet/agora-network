import { FormEvent, useEffect, useMemo, useState } from "react";
import {
  ACADEMY_CERTIFICATE,
} from "../../../shared/academy";
import { GRANT_DISBURSEMENT } from "../../../shared/grants";
import { recordVote } from "../../../shared/assembly";
import {
  PUSH_TRANSPORT,
  advanceDrcPay,
  broadcastDrcPay,
  createCommunityClient,
  defaultNotificationPrefs,
  GOVERNANCE_AREAS,
  inboxNotice,
  merchantForAddress,
  notificationBody,
  preferChainTreasuries,
  proposalBadge,
  recordLessonProgress,
  searchHubs,
  signDrcPayIntent,
  signSessionChallenge,
  transitionMission,
  voteEligibility,
  parseAgoraQr,
  type CommunityClient,
  type CommunitySession,
  type DataSource,
  type DrcPayReceipt,
  type DrcPayStep,
  type InAppNotice,
  type Mission,
  type NotificationPrefs,
  type OfflineView,
  type PublicPassport,
  type WalletMode,
} from "../../../shared/community";
import { submitInspectedDrcPayment } from "../../../shared/community/nodePay";
import {
  deriveAccount,
  type LightClient,
  type LightProtocolTreasuries,
} from "../../../shared/light-client";

export type CommunitySpendContext = {
  mode: WalletMode;
  mnemonic: string | null;
  network: string | null;
  genesisHash: string | null;
  chainId: string | null;
  light: LightClient | null;
};

const fieldStyle = {
  width: "100%",
  padding: "0.65rem 0.75rem",
  border: "1px solid color-mix(in srgb, var(--agora-gold) 35%, transparent)",
  background: "transparent",
  color: "var(--agora-ink)",
  fontFamily: "ui-monospace, monospace",
  fontSize: "0.85rem",
} as const;

const btnStyle = {
  padding: "0.55rem 0.9rem",
  border: "1px solid var(--agora-gold)",
  background: "transparent",
  color: "var(--agora-gold)",
  cursor: "pointer",
  fontFamily: "var(--agora-display)",
} as const;

function Source({ source }: { source: DataSource | string }) {
  return (
    <span className="agora-source" title="Data source">
      {source}
    </span>
  );
}

function Banner({ view }: { view: { label: string; confirmed: false } | null }) {
  if (!view) return <p className="agora-meta">Loading community records…</p>;
  return (
    <p className="agora-meta">
      {view.label}
      {view.confirmed ? "" : " · confirmed: no"}
    </p>
  );
}

function Empty({ text }: { text: string }) {
  return <p className="agora-meta">{text}</p>;
}

export function useCommunityClient(baseUrl?: string | null): CommunityClient {
  return useMemo(() => createCommunityClient({ baseUrl: baseUrl ?? null }), [baseUrl]);
}

export function PassportScreen({
  client,
  address,
  spend,
}: {
  client: CommunityClient;
  address: string | null;
  spend: CommunitySpendContext;
}) {
  const [view, setView] = useState<OfflineView<PublicPassport | null> | null>(null);
  const [email, setEmail] = useState("");
  const [language, setLanguage] = useState("en");
  const [privateNote, setPrivateNote] = useState("Private profile stays on this device.");
  const [session, setSession] = useState<CommunitySession | null>(null);
  const [sessionNote, setSessionNote] = useState(
    "Sign the infrastructure challenge with the vault key. A pasted bearer token is not accepted.",
  );

  useEffect(() => {
    void client.passport(address ?? undefined).then(setView);
  }, [client, address]);

  const passport = view?.data;
  return (
    <section className="agora-block">
      <p className="agora-eyebrow">Passport</p>
      <h2>Identity credential</h2>
      <p className="agora-meta">
        Non-transferable reputation credential. Not a financial token. Wallet keys stay in the vault.
      </p>
      <Banner view={view} />
      {!passport ? (
        <Empty text="No public passport for this address. The profile is empty, not hidden." />
      ) : (
        <div className="agora-grid">
          <p>Username <strong>{passport.username}</strong> <Source source={passport.source} /></p>
          <p>Address <span className="agora-mono">{passport.address}</span> <Source source="community submitted" /></p>
          <p>Avatar hash {passport.avatarHash ?? "none"} <Source source={passport.source} /></p>
          <p>Roles {passport.roles.join(", ") || "none"} <Source source={passport.source} /></p>
          <p>Overall {passport.overall} <Source source={passport.source} /></p>
          {Object.entries(passport.reputations).map(([category, score]) => (
            <p key={category}>
              {category} {score} <Source source={passport.source} />
            </p>
          ))}
          <p>
            Badges {passport.badges.map((badge) => badge.name).join(", ") || "none"} · non-transferable{" "}
            <Source source={passport.badges[0]?.source ?? passport.source} />
          </p>
          <p>
            Contributions {Object.entries(passport.contributionCounts).map(([k, n]) => `${k} ${n}`).join(" · ") || "none"}{" "}
            <Source source={passport.source} />
          </p>
        </div>
      )}
      <form
        className="agora-form"
        onSubmit={(event) => {
          event.preventDefault();
          setPrivateNote(
            `Saved locally (${language}${email ? ", email on device" : ""}). consensus: false. Not sent to a chain.`,
          );
        }}
      >
        <p className="agora-eyebrow">Private profile</p>
        <input
          value={email}
          onChange={(event) => setEmail(event.target.value)}
          placeholder="email (local only)"
          aria-label="Email"
          style={fieldStyle}
        />
        <input
          value={language}
          onChange={(event) => setLanguage(event.target.value)}
          aria-label="Language"
          style={fieldStyle}
        />
        <button type="submit" style={btnStyle}>Save on device</button>
        <p className="agora-meta">{privateNote} <Source source="community submitted" /></p>
      </form>
      <div className="agora-form">
        <p className="agora-eyebrow">Community session</p>
        <button
          type="button"
          style={btnStyle}
          onClick={() => {
            void (async () => {
              if (spend.mode !== "signing" || !spend.mnemonic?.trim() || !spend.network) {
                setSessionNote("Unlock a spend wallet on this device. Watch-only cannot sign, and a pasted token is refused.");
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
                  `Session for ${opened.address} expires ${new Date(opened.expiresAt).toLocaleString()}. Private profile storage: ${profile.storage}. consensus: ${profile.consensus}.`,
                );
              } catch (err) {
                setSession(null);
                setSessionNote(err instanceof Error ? err.message : "session refused");
              }
            })();
          }}
        >
          Sign session with vault key
        </button>
        <p className="agora-meta">
          {sessionNote}
          {session ? ` · scopes ${session.scopes.join(", ")}` : ""}
        </p>
      </div>
    </section>
  );
}

function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

export function DrcPayScreen({
  client,
  spend,
}: {
  client: CommunityClient;
  spend: CommunitySpendContext;
}) {
  const [raw, setRaw] = useState("");
  const [step, setStep] = useState<DrcPayStep>("scan");
  const [note, setNote] = useState("Paste a versioned Agora QR. The PC wallet has no camera. Inspect destination and amount before signing.");
  const [fee, setFee] = useState("1");
  const [nonce, setNonce] = useState("0");
  const [receipt, setReceipt] = useState<DrcPayReceipt | null>(null);
  const mode = spend.mode;

  function onInspect(event: FormEvent) {
    event.preventDefault();
    const parsed = parseAgoraQr(raw);
    if (!parsed.ok) {
      setNote(parsed.error);
      setStep("scan");
      return;
    }
    if (parsed.preview.kind !== "drc-payment" && parsed.preview.kind !== "merchant-pay") {
      setNote("This QR is not a DRC payment.");
      return;
    }
    setStep("inspect");
    setNote(`${parsed.preview.summary}. Inspect the fields, then continue.`);
  }

  const parsed = parseAgoraQr(raw);
  const preview = parsed.ok ? parsed.preview : null;

  return (
    <section className="agora-block">
      <p className="agora-eyebrow">DRC PAY</p>
      <h2>Scan, verify, then sign</h2>
      <p className="agora-meta">Move value. The destination and amount stay on screen before a signature.</p>
      <p className="agora-meta">Step {step}</p>
      <form onSubmit={onInspect} className="agora-form">
        <textarea
          value={raw}
          onChange={(event) => setRaw(event.target.value)}
          placeholder="agora:1:drc-payment?to=…&amount=…"
          aria-label="DRC QR"
          rows={3}
          style={fieldStyle}
        />
        <button type="submit" style={btnStyle}>Inspect QR</button>
      </form>
      {preview?.destination && preview.amount ? (
        <div className="agora-grid">
          <p>Destination <span className="agora-mono">{preview.destination}</span> <Source source="community submitted" /></p>
          <p>Amount {preview.amount} DRC <Source source="community submitted" /></p>
          <p>Wallet mode {mode}. {mode === "watch-only" ? "Watch-only cannot spend." : "The vault key signs. It is not pasted."}</p>
          <label>
            Fee{" "}
            <input value={fee} onChange={(event) => setFee(event.target.value)} aria-label="DRC fee" style={fieldStyle} />
          </label>
          <label>
            Nonce{" "}
            <input value={nonce} onChange={(event) => setNonce(event.target.value)} aria-label="DRC nonce" style={fieldStyle} />
          </label>
          <div className="agora-actions">
            {step === "inspect" ? (
              <button
                type="button"
                style={btnStyle}
                onClick={() => {
                  void client.merchants().then((view) => {
                    const merchant = merchantForAddress(view.data ?? [], preview.destination!);
                    setStep(advanceDrcPay("inspect"));
                    setNote(
                      merchant
                        ? `Merchant ${merchant.displayName} listed. ${merchant.source}. No merchant keys.`
                        : "Payee is not in the merchant directory. Destination stays visible.",
                    );
                  });
                }}
              >
                Verify merchant
              </button>
            ) : null}
            {step === "verify-merchant" ? (
              <button
                type="button"
                style={btnStyle}
                onClick={() => {
                  setStep(advanceDrcPay("verify-merchant"));
                  setNote(`Confirm ${preview.amount} DRC to ${preview.destination} before signing.`);
                }}
              >
                Confirm {preview.amount} to {preview.destination}
              </button>
            ) : null}
            {step === "confirm" ? (
              <button
                type="button"
                style={btnStyle}
                onClick={() => {
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
                Sign
              </button>
            ) : null}
            {step === "sign" ? (
              <button
                type="button"
                style={btnStyle}
                onClick={() => {
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
                      if (
                        mode === "signing" &&
                        spend.mnemonic &&
                        spend.light &&
                        spend.network &&
                        spend.genesisHash
                      ) {
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
                Broadcast
              </button>
            ) : null}
          </div>
        </div>
      ) : null}
      <p className="agora-meta">{note}</p>
      {receipt ? (
        <p className="agora-meta">
          Broadcast {receipt.broadcast} · confirmed {receipt.confirmed ? "yes, node receipt" : "no"}
          {receipt.paymentId ? ` · ${receipt.paymentId}` : ""}
        </p>
      ) : null}
    </section>
  );
}

export function CommunityScreens({
  lane,
  client,
  address,
  chainTreasuries,
  notifications,
  onNotifications,
  spend,
}: {
  lane: string;
  client: CommunityClient;
  address: string | null;
  chainTreasuries: LightProtocolTreasuries | null;
  notifications: NotificationPrefs;
  onNotifications: (next: NotificationPrefs) => void;
  spend: CommunitySpendContext;
}) {
  const [missions, setMissions] = useState<Mission[]>([]);
  const [missionLabel, setMissionLabel] = useState("Loading missions…");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void client.missions().then((view) => {
      if (cancelled) return;
      setMissions(view.data ?? []);
      setMissionLabel(view.label);
    }).catch((err: unknown) => {
      if (!cancelled) setError(err instanceof Error ? err.message : "community read failed");
    });
    return () => {
      cancelled = true;
    };
  }, [client]);

  if (lane === "PASSPORT") return <PassportScreen client={client} address={address} spend={spend} />;
  if (lane === "DRC") return <DrcPayScreen client={client} spend={spend} />;
  if (lane === "SWAP") {
    return (
      <section className="agora-block">
        <p className="agora-eyebrow">Swap</p>
        <h2>Unavailable</h2>
        <p>No DEX, AMM, or bridge is wired in this light client.</p>
        <Source source="external" />
      </section>
    );
  }
  if (lane === "MISSIONS") {
    return (
      <section className="agora-block">
        <p className="agora-eyebrow">Missions</p>
        <h2>AVAILABLE to COMPLETED</h2>
        <p className="agora-meta">{missionLabel} · confirmed: no</p>
        {error ? <p className="agora-meta">{error}</p> : null}
        {missions.length === 0 ? <Empty text="No missions. The board is empty." /> : null}
        <ul className="agora-list">
          {missions.map((mission) => (
            <li key={mission.id}>
              <strong>{mission.title}</strong> · {mission.state} · {mission.rewardNote}{" "}
              <Source source={mission.source} />
              {mission.state !== "COMPLETED" ? (
                <button
                  type="button"
                  style={btnStyle}
                  onClick={() => {
                    const order = ["AVAILABLE", "ACCEPTED", "IN_PROGRESS", "SUBMITTED", "COMPLETED"] as const;
                    const next = order[order.indexOf(mission.state) + 1];
                    if (!next) return;
                    void client.advanceMission(mission.id, next).then((result) => {
                      setMissionLabel(result.label);
                      if (!result.data?.recorded) return;
                      setMissions((rows) =>
                        rows.map((row) =>
                          row.id === mission.id ? { ...row, state: transitionMission(row.state, next) } : row,
                        ),
                      );
                    });
                  }}
                >
                  Advance
                </button>
              ) : null}
            </li>
          ))}
        </ul>
      </section>
    );
  }
  return (
    <ModuleScreen
      lane={lane}
      client={client}
      address={address}
      chainTreasuries={chainTreasuries}
      notifications={notifications}
      onNotifications={onNotifications}
      spend={spend}
    />
  );
}

function ModuleScreen({
  lane,
  client,
  address,
  chainTreasuries,
  notifications,
  onNotifications,
  spend,
}: {
  lane: string;
  client: CommunityClient;
  address: string | null;
  chainTreasuries: LightProtocolTreasuries | null;
  notifications: NotificationPrefs;
  onNotifications: (next: NotificationPrefs) => void;
  spend: CommunitySpendContext;
}) {
  const [label, setLabel] = useState("Loading…");
  const [body, setBody] = useState<string[]>([]);
  const [query, setQuery] = useState("");
  const [progressNote, setProgressNote] = useState(
    `Academy progress is stored on this device. Certificate: ${ACADEMY_CERTIFICATE.status}. ${ACADEMY_CERTIFICATE.reason}`,
  );
  const [inbox, setInbox] = useState<InAppNotice[]>([]);
  const [voteNote, setVoteNote] = useState("Vote results stay PLANNED.");

  useEffect(() => {
    let cancelled = false;
    async function load() {
      if (lane === "COMMUNITY" || lane === "HOME") {
        const [eco, forum, hubs, developers] = await Promise.all([
          client.ecosystem(),
          client.forum(),
          client.hubs(),
          client.developers(),
        ]);
        if (cancelled) return;
        setLabel(eco.label);
        const filtered = searchHubs(hubs.data ?? [], { region: query, name: query });
        setBody([
          `Passports ${eco.data?.passportCount ?? 0} · missions ${eco.data?.missionCount ?? 0} · merchants ${eco.data?.merchantCount ?? 0}`,
          ...(forum.data ?? []).map((post) => `Forum ${post.category}: ${post.title} by ${post.authorUsername}`),
          ...filtered.map((hub) => `Hub ${hub.name} · ${hub.region}`),
          ...(developers.data ?? []).map((dev) => `Developer ${dev.name} · ${dev.focus}`),
          query ? `Region search “${query}”. Location is typed, not GPS.` : "Type a region to filter hubs. GPS is not requested.",
        ]);
      } else if (lane === "ACADEMY") {
        const view = await client.academy();
        if (cancelled) return;
        setLabel(view.label);
        setBody(
          (view.data?.courses ?? []).flatMap((course) => [
            `${course.track}: ${course.title}`,
            ...course.lessons.map((lesson) => `Lesson ${lesson.title}`),
          ]),
        );
      } else if (lane === "GRANTS") {
        const view = await client.grants();
        if (cancelled) return;
        setLabel(view.label);
        setBody((view.data ?? []).map((grant) => `${grant.title} · ${grant.asset} ${grant.total} · disbursement ${GRANT_DISBURSEMENT} · disbursesFunds ${grant.disbursesFunds}`));
      } else if (lane === "BOUNTIES") {
        const view = await client.bounties();
        if (cancelled) return;
        setLabel(view.label);
        setBody((view.data ?? []).map((bounty) => `${bounty.title} · ${bounty.status}`));
      } else if (lane === "GUILDS") {
        const view = await client.guilds();
        if (cancelled) return;
        setLabel(view.label);
        setBody((view.data ?? []).map((guild) => `${guild.kind} · ${guild.charter} · members ${guild.memberCount}`));
      } else if (lane === "MERCHANTS") {
        const view = await client.merchants();
        if (cancelled) return;
        setLabel(view.label);
        setBody((view.data ?? []).map((merchant) => `${merchant.displayName} · ${merchant.drcAddress} · keys stored: ${merchant.holdsMerchantKeys}`));
      } else if (lane === "EVENTS") {
        const view = await client.events();
        if (cancelled) return;
        setLabel(view.label);
        setBody((view.data ?? []).map((item) => `${item.title} · ${item.region} · ${item.startsAt}`));
      } else if (lane === "ASSEMBLY") {
        const view = await client.proposals();
        if (cancelled) return;
        setLabel(view.label);
        const passport = (await client.passport(address ?? undefined)).data;
        setBody([
          ...GOVERNANCE_AREAS.map((area) => {
            const gate = passport
              ? voteEligibility(area.id, passport)
              : { eligible: false, label: area.eligibility };
            return `${area.label}: ${gate.label}`;
          }),
          ...(view.data ?? []).map((proposal) => {
            const badge = proposalBadge(proposal);
            return `${badge.badge} · ${proposal.title} · ${badge.note}`;
          }),
          "Vote results: PLANNED. This screen does not tally votes or submit a governance transaction.",
          "Civic RPC below is administrative local state, not on-chain governance.",
        ]);
      } else if (lane === "TREASURY") {
        const view = await client.treasury();
        if (cancelled) return;
        const rows = preferChainTreasuries(view.data ?? [], chainTreasuries);
        setLabel(chainTreasuries ? "full node agora_getProtocolTreasuries · not a header proof" : view.label);
        setBody(rows.map((row) => `${row.id} · ${row.asset} ${row.balance} · ${row.source} · ${row.note}`));
      } else if (lane === "SETTINGS") {
        setLabel("notification preferences stay on device");
        const sample = notificationBody("Assembly", "New advisory poll. Balance 10 DRC");
        setBody([
          `Missions ${notifications.missions ? "on" : "off"}`,
          `Assembly ${notifications.assembly ? "on" : "off"}`,
          `Grants ${notifications.grants ? "on" : "off"}`,
          `Merchants ${notifications.merchants ? "on" : "off"}`,
          `Amounts in push: ${notifications.includeAmounts ? "on" : "off"}`,
          `Push transport: ${PUSH_TRANSPORT}. No APNs or FCM. In-app inbox is the delivery path.`,
          `Sample inbox copy: ${sample.body}`,
        ]);
      } else if (lane === "OVL") {
        setLabel("OVL BUILD");
        setBody(["Programmable execution lives on the full node. This screen does not invent contracts or a swap."]);
      } else if (lane === "TLT") {
        setLabel("TLT SECURE");
        setBody(["UTXO and PoW stay on the full node. This phone or PC does not mine."]);
      }
    }
    void load().catch((err: unknown) => {
      if (!cancelled) setBody([err instanceof Error ? err.message : "offline"]);
    });
    return () => {
      cancelled = true;
    };
  }, [lane, client, address, chainTreasuries, notifications, query]);

  return (
    <section className="agora-block">
      <p className="agora-eyebrow">{lane}</p>
      <p className="agora-meta">{label} · confirmed: no</p>
      {(lane === "COMMUNITY" || lane === "HOME") && (
        <form
          className="agora-form"
          onSubmit={(event) => {
            event.preventDefault();
            setQuery((event.currentTarget.elements.namedItem("region") as HTMLInputElement).value);
          }}
        >
          <input name="region" placeholder="region or hub name" aria-label="Hub search" style={fieldStyle} />
          <button type="submit" style={btnStyle}>Search hubs</button>
        </form>
      )}
      {lane === "ACADEMY" ? (
        <button
          type="button"
          style={btnStyle}
          onClick={() => {
            const next = recordLessonProgress([], "course-assets", "lesson-drc");
            setProgressNote(
              `Local progress ${next[0]?.completedLessonIds.join(", ")}. Certificate ${ACADEMY_CERTIFICATE.status}. ${ACADEMY_CERTIFICATE.reason}`,
            );
          }}
        >
          Mark DRC lesson read
        </button>
      ) : null}
      {lane === "COMMUNITY" ? <ForumReports client={client} address={address} /> : null}
      {lane === "ASSEMBLY" ? (
        <button
          type="button"
          style={btnStyle}
          onClick={() => {
            try {
              recordVote();
            } catch (err) {
              setVoteNote(err instanceof Error ? err.message : "vote refused");
            }
          }}
        >
          Record vote
        </button>
      ) : null}
      {lane === "ASSEMBLY" ? <p className="agora-meta">{voteNote}</p> : null}
      {body.length === 0 ? <Empty text="Nothing to show. This module is empty or still loading." /> : null}
      <ul className="agora-list">
        {body.map((line) => (
          <li key={line}>{line}</li>
        ))}
      </ul>
      {lane === "ACADEMY" ? <p className="agora-meta">{progressNote}</p> : null}
      {lane === "SETTINGS" ? (
        <button
          type="button"
          style={btnStyle}
          onClick={() => onNotifications({ ...notifications, missions: !notifications.missions, includeAmounts: false })}
        >
          Toggle mission notices
        </button>
      ) : null}
      {lane === "SETTINGS" ? (
        <button
          type="button"
          style={btnStyle}
          onClick={() => {
            const notice = inboxNotice("Assembly", "New advisory poll. Balance 10 DRC", Date.now());
            setInbox((rows) => [...rows, notice]);
          }}
        >
          Save notice to inbox
        </button>
      ) : null}
      {lane === "SETTINGS"
        ? inbox.map((notice) => (
            <p key={notice.id} className="agora-meta">
              Inbox · {notice.title}: {notice.body}
            </p>
          ))
        : null}
    </section>
  );
}

function ForumReports({ client, address }: { client: CommunityClient; address: string | null }) {
  const [note, setNote] = useState("Replies go to the infrastructure forum. This app does not host one.");
  const [postId, setPostId] = useState("");
  const [body, setBody] = useState("");
  return (
    <form
      className="agora-form"
      onSubmit={(event) => {
        event.preventDefault();
        if (!address) {
          setNote("Unlock or derive an address before replying. The device will not invent an author.");
          return;
        }
        void client.replyToForum({
          postId,
          body,
          authorAddress: address,
        }).then((result) => setNote(result.label));
      }}
    >
      <input
        value={postId}
        onChange={(event) => setPostId(event.target.value)}
        placeholder="post id"
        aria-label="Forum post id"
        style={fieldStyle}
      />
      <textarea
        value={body}
        onChange={(event) => setBody(event.target.value)}
        placeholder="Reply"
        aria-label="Forum reply"
        rows={3}
        style={fieldStyle}
      />
      <button type="submit" style={btnStyle}>Post reply</button>
      <button
        type="button"
        style={btnStyle}
        onClick={() => {
          void client.forum().then(async (view) => {
            const id = view.data?.[0]?.id;
            if (!id) {
              setNote("No post to report.");
              return;
            }
            setPostId(id);
            const report = await client.reportForum(id, "community report");
            setNote(report.label);
          });
        }}
      >
        Report first post
      </button>
      <p className="agora-meta">{note}</p>
    </form>
  );
}

export function initialNotificationPrefs(): NotificationPrefs {
  return defaultNotificationPrefs();
}
