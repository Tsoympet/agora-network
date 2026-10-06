import { presentCached, type OfflineView } from "./cache.ts";
import { communityFixtures, demoPrivateProfile } from "./fixtures.ts";
import {
  createDeviceInfrastructureClient,
  type DataPlane,
  type InfrastructureFetch,
} from "../data-plane/deviceClient.ts";
import type {
  AcademyCatalog,
  Bounty,
  CommunityBundle,
  CommunityEvent,
  CommunityProposal,
  CommunityTreasuryRow,
  Contribution,
  DirectoryEntry,
  EcosystemSnapshot,
  ForumPost,
  Grant,
  Guild,
  Hub,
  MerchantProfile,
  Mission,
  PrivatePassportProfile,
  PublicPassport,
} from "./types.ts";

export type CommunityClient = {
  baseUrl: string | null;
  online: () => boolean;
  passport: (address?: string) => Promise<OfflineView<PublicPassport | null>>;
  privateProfile: (sessionToken: string | null) => Promise<PrivatePassportProfile>;
  reputation: (address: string) => Promise<OfflineView<PublicPassport["reputations"] | null>>;
  missions: () => Promise<OfflineView<Mission[]>>;
  grants: () => Promise<OfflineView<Grant[]>>;
  bounties: () => Promise<OfflineView<Bounty[]>>;
  academy: () => Promise<OfflineView<AcademyCatalog>>;
  events: () => Promise<OfflineView<CommunityEvent[]>>;
  merchants: () => Promise<OfflineView<MerchantProfile[]>>;
  guilds: () => Promise<OfflineView<Guild[]>>;
  proposals: () => Promise<OfflineView<CommunityProposal[]>>;
  treasury: () => Promise<OfflineView<CommunityTreasuryRow[]>>;
  contributions: (address?: string) => Promise<OfflineView<Contribution[]>>;
  hubs: () => Promise<OfflineView<Hub[]>>;
  forum: () => Promise<OfflineView<ForumPost[]>>;
  developers: () => Promise<OfflineView<DirectoryEntry[]>>;
  ecosystem: () => Promise<OfflineView<EcosystemSnapshot>>;
  reportForum: (postId: string, reason: string) => Promise<OfflineView<{ dispatched: boolean }>>;
  advanceMission: (id: string, to: string) => Promise<OfflineView<{ recorded: boolean }>>;
};

function serviceFor(path: string): string {
  const pathname = path.split("?")[0] ?? path;
  if (pathname === "/forum") return "forum";
  if (pathname === "/grants" || pathname === "/missions" || pathname === "/missions/advance") {
    return "grant-admin";
  }
  if (pathname === "/events") return "event-service";
  if (pathname === "/search") return "search";
  if (pathname.startsWith("/moderation")) return "moderation";
  if (pathname.startsWith("/notifications")) return "notification-dispatch";
  if (pathname.startsWith("/v1/indexer")) return "indexer";
  return "community";
}

export function createCommunityClient(options: {
  baseUrl?: string | null;
  fetchImpl?: InfrastructureFetch;
  sessionToken?: string | null;
  bundle?: CommunityBundle;
}): CommunityClient {
  const bundle = options.bundle ?? communityFixtures();
  const infra = createDeviceInfrastructureClient({
    baseUrl: options.baseUrl,
    fetchImpl: options.fetchImpl,
  });
  const baseUrl = infra.baseUrl;
  let lastOnline = false;

  function view<T>(plane: DataPlane, online: boolean, data: T, label?: string): OfflineView<T> {
    lastOnline = online && baseUrl !== null;
    const shown = presentCached(lastOnline, data, !lastOnline, plane);
    if (label) shown.label = label;
    else if (!baseUrl) shown.label = "local fixture · community submitted · not confirmed";
    return shown;
  }

  async function load<T>(plane: DataPlane, path: string, fallback: T, sessionToken: string | null): Promise<OfflineView<T>> {
    if (!infra.configured) return view(plane, false, fallback);
    try {
      const data = await infra.facades.get<T>(serviceFor(path), path, sessionToken);
      return view(plane, true, data);
    } catch {
      return view(plane, false, fallback);
    }
  }

  return {
    baseUrl,
    online: () => lastOnline,
    async passport(address) {
      const fallback =
        bundle.passports.find((row) => !address || row.address === address) ??
        bundle.passports[0] ??
        null;
      return load(
        "infrastructure",
        `/passport${address ? `?address=${encodeURIComponent(address)}` : ""}`,
        fallback,
        options.sessionToken ?? null,
      );
    },
    async privateProfile(sessionToken) {
      if (!sessionToken) throw new Error("private profile requires a community session");
      const result = await load<PrivatePassportProfile>(
        "device",
        "/passport/private",
        { ...demoPrivateProfile, storage: "local" },
        sessionToken,
      );
      const profile = result.data ?? { ...demoPrivateProfile, storage: "local" as const };
      profile.consensus = false;
      if (!result.online) profile.storage = "local";
      return profile;
    },
    async reputation(address) {
      const passport = bundle.passports.find((row) => row.address === address) ?? null;
      return load(
        "infrastructure",
        `/reputation?address=${encodeURIComponent(address)}`,
        passport?.reputations ?? null,
        options.sessionToken ?? null,
      );
    },
    missions() {
      return load("infrastructure", "/missions", bundle.missions, null);
    },
    grants() {
      return load("infrastructure", "/grants", bundle.grants, null);
    },
    bounties() {
      return load("infrastructure", "/bounties", bundle.bounties, null);
    },
    academy() {
      return load("infrastructure", "/academy", bundle.academy, null);
    },
    events() {
      return load("infrastructure", "/events", bundle.events, null);
    },
    merchants() {
      return load("infrastructure", "/merchants", bundle.merchants, null);
    },
    guilds() {
      return load("infrastructure", "/guilds", bundle.guilds, null);
    },
    proposals() {
      return load("infrastructure", "/proposals", bundle.proposals, null);
    },
    treasury() {
      return Promise.resolve(view(
        "on-chain",
        false,
        [] as CommunityTreasuryRow[],
        "treasury balances come from a full node · this client does not invent them",
      ));
    },
    contributions() {
      return load("infrastructure", "/contributions", bundle.contributions, null);
    },
    hubs() {
      return load("infrastructure", "/hubs", bundle.hubs, null);
    },
    forum() {
      return load("infrastructure", "/forum", bundle.forum, null);
    },
    developers() {
      return load("infrastructure", "/developers", bundle.developers, null);
    },
    ecosystem() {
      return load("infrastructure", "/ecosystem", bundle.ecosystem, null);
    },
    async reportForum(postId, reason) {
      if (!infra.configured) {
        return view(
          "infrastructure",
          false,
          { dispatched: false },
          "moderation runs on infrastructure · this device did not dispatch a report",
        );
      }
      try {
        await infra.facades.moderation.report({ postId, reason });
        return view(
          "infrastructure",
          true,
          { dispatched: true },
          "report accepted by the moderation service · not a consensus action",
        );
      } catch {
        return view(
          "infrastructure",
          false,
          { dispatched: false },
          "moderation service unreachable · report not dispatched",
        );
      }
    },
    async advanceMission(id, to) {
      if (!infra.configured) {
        return view(
          "infrastructure",
          false,
          { recorded: false },
          "mission admin runs on infrastructure · this device did not record the transition",
        );
      }
      try {
        const recorded = await infra.facades.grants.advance({ id, to });
        return view(
          "infrastructure",
          true,
          recorded,
          "mission service recorded the transition · not a consensus transaction",
        );
      } catch {
        return view(
          "infrastructure",
          false,
          { recorded: false },
          "mission service unreachable · transition not recorded",
        );
      }
    },
  };
}

export function preferChainTreasuries(
  _community: CommunityTreasuryRow[],
  chain:
    | {
        consensus_mutations_active?: boolean;
        treasuries?: Array<{ id: string; asset: "TLT" | "OVL" | "DRC"; balance: { base_units?: string } | string | number }>;
      }
    | null,
): CommunityTreasuryRow[] {
  // Community fixtures are not treasury balances. Only a full-node read is shown.
  if (!chain?.treasuries?.length) return [];
  return chain.treasuries.map((row) => {
    const balance =
      typeof row.balance === "string" || typeof row.balance === "number"
        ? String(row.balance)
        : (row.balance.base_units ?? "0");
    return {
      id: row.id,
      asset: row.asset,
      balance,
      source: "indexed",
      note: chain.consensus_mutations_active
        ? "Full-node canonical treasury read. Not the infrastructure indexer and not a header proof."
        : "Full-node treasury scaffold. Spend mutations are not active. Not the infrastructure indexer and not a header proof.",
    };
  });
}
