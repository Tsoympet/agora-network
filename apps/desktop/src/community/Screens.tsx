import { FormEvent, useEffect, useMemo, useState } from "react";
import {
  advanceDrcPay,
  broadcastDrcPay,
  createCommunityClient,
  defaultNotificationPrefs,
  GOVERNANCE_AREAS,
  merchantForAddress,
  notificationBody,
  preferChainTreasuries,
  proposalBadge,
  recordLessonProgress,
  reportForumPost,
  searchHubs,
  signDrcPayIntent,
  transitionMission,
  voteEligibility,
  parseAgoraQr,
  type CommunityClient,
  type DataSource,
  type DrcPayStep,
  type Mission,
  type NotificationPrefs,
  type OfflineView,
  type PublicPassport,
} from "../../../shared/community";
import type { LightProtocolTreasuries } from "../../../shared/light-client";

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
}: {
  client: CommunityClient;
  address: string | null;
}) {
  const [view, setView] = useState<OfflineView<PublicPassport | null> | null>(null);
  const [email, setEmail] = useState("");
  const [language, setLanguage] = useState("en");
  const [privateNote, setPrivateNote] = useState("Private profile stays on this device.");

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
    </section>
  );
}

export function DrcPayScreen({ client }: { client: CommunityClient }) {
  const [raw, setRaw] = useState("");
  const [step, setStep] = useState<DrcPayStep>("scan");
  const [note, setNote] = useState("Paste a versioned Agora QR. Camera capture is not part of this slice.");
  const [mode, setMode] = useState<"signing" | "watch-only">("signing");

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
          <label>
            Wallet mode{" "}
            <select value={mode} onChange={(event) => setMode(event.target.value as "signing" | "watch-only")}>
              <option value="signing">signing</option>
              <option value="watch-only">watch-only</option>
            </select>
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
                    setStep(advanceDrcPay("confirm"));
                    setNote("Signing wallet accepted the intent. The seed stayed in the vault. Consensus DRC bytes are not built here.");
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
                  try {
                    const receipt = broadcastDrcPay({ mode, preview, signed: true });
                    setStep(advanceDrcPay("sign"));
                    setStep(advanceDrcPay("broadcast"));
                    setNote(receipt.note);
                  } catch (err) {
                    setNote(err instanceof Error ? err.message : "broadcast refused");
                  }
                }}
              >
                Broadcast
              </button>
            ) : null}
          </div>
        </div>
      ) : null}
      <p className="agora-meta">{note}</p>
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
}: {
  lane: string;
  client: CommunityClient;
  address: string | null;
  chainTreasuries: LightProtocolTreasuries | null;
  notifications: NotificationPrefs;
  onNotifications: (next: NotificationPrefs) => void;
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

  if (lane === "PASSPORT") return <PassportScreen client={client} address={address} />;
  if (lane === "DRC") return <DrcPayScreen client={client} />;
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
                    setMissions((rows) =>
                      rows.map((row) =>
                        row.id === mission.id ? { ...row, state: transitionMission(row.state, next) } : row,
                      ),
                    );
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
}: {
  lane: string;
  client: CommunityClient;
  address: string | null;
  chainTreasuries: LightProtocolTreasuries | null;
  notifications: NotificationPrefs;
  onNotifications: (next: NotificationPrefs) => void;
}) {
  const [label, setLabel] = useState("Loading…");
  const [body, setBody] = useState<string[]>([]);
  const [query, setQuery] = useState("");
  const [progressNote, setProgressNote] = useState("Academy progress is stored on this device.");

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
        setBody((view.data ?? []).map((grant) => `${grant.title} · ${grant.asset} ${grant.total} · disburses ${grant.disbursesFunds}`));
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
          "Civic RPC below is administrative local state, not on-chain governance.",
        ]);
      } else if (lane === "TREASURY") {
        const view = await client.treasury();
        if (cancelled) return;
        const rows = preferChainTreasuries(view.data ?? [], chainTreasuries);
        setLabel(chainTreasuries ? "indexed from agora_getProtocolTreasuries · not a header proof" : view.label);
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
          `Sample push: ${sample.body}`,
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
            setProgressNote(`Local progress ${next[0]?.completedLessonIds.join(", ")}. Not an on-chain certificate.`);
          }}
        >
          Mark DRC lesson read
        </button>
      ) : null}
      {lane === "COMMUNITY" ? <ForumReports client={client} /> : null}
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
    </section>
  );
}

function ForumReports({ client }: { client: CommunityClient }) {
  const [note, setNote] = useState("Report stays a moderation hook. It does not change reputation.");
  return (
    <button
      type="button"
      style={btnStyle}
      onClick={() => {
        void client.forum().then((view) => {
          const posts = reportForumPost(view.data ?? [], view.data?.[0]?.id ?? "");
          setNote(`Reports on first post: ${posts[0]?.reportCount ?? 0}. ${posts[0]?.source ?? "community submitted"}.`);
        });
      }}
    >
      Report first post · {note}
    </button>
  );
}

export function initialNotificationPrefs(): NotificationPrefs {
  return defaultNotificationPrefs();
}
