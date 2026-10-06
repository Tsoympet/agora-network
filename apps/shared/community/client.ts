import { presentCached, type OfflineView } from "./cache.ts";
import { communityFixtures, demoPrivateProfile } from "./fixtures.ts";
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
};

type FetchLike = (input: string, init?: { headers?: Record<string, string> }) => Promise<{
  ok: boolean;
  json: () => Promise<unknown>;
}>;

async function read<T>(
  baseUrl: string | null,
  path: string,
  fetchImpl: FetchLike,
  sessionToken: string | null,
  fallback: T,
): Promise<{ online: boolean; data: T }> {
  if (!baseUrl) return { online: false, data: fallback };
  try {
    const headers: Record<string, string> = {};
    if (sessionToken) headers.authorization = `Bearer ${sessionToken}`;
    const response = await fetchImpl(`${baseUrl}${path}`, { headers });
    if (!response.ok) return { online: false, data: fallback };
    return { online: true, data: (await response.json()) as T };
  } catch {
    return { online: false, data: fallback };
  }
}

export function createCommunityClient(options: {
  baseUrl?: string | null;
  fetchImpl?: FetchLike;
  sessionToken?: string | null;
  bundle?: CommunityBundle;
}): CommunityClient {
  const bundle = options.bundle ?? communityFixtures();
  const fetchImpl = options.fetchImpl ?? (globalThis.fetch as FetchLike);
  const baseUrl = options.baseUrl?.replace(/\/$/, "") || null;
  let lastOnline = baseUrl === null ? false : true;

  function view<T>(online: boolean, data: T): OfflineView<T> {
    lastOnline = online && baseUrl !== null;
    const shown = presentCached(lastOnline, data, !lastOnline);
    if (!baseUrl) {
      shown.label = "local fixture · community submitted · not confirmed";
    }
    return shown;
  }

  return {
    baseUrl,
    online: () => lastOnline,
    async passport(address) {
      const fallback =
        bundle.passports.find((row) => !address || row.address === address) ??
        bundle.passports[0] ??
        null;
      const result = await read<PublicPassport | null>(
        baseUrl,
        `/passport${address ? `?address=${encodeURIComponent(address)}` : ""}`,
        fetchImpl,
        options.sessionToken ?? null,
        fallback,
      );
      return view(result.online, result.data);
    },
    async privateProfile(sessionToken) {
      if (!sessionToken) throw new Error("private profile requires a community session");
      const result = await read<PrivatePassportProfile>(
        baseUrl,
        "/passport/private",
        fetchImpl,
        sessionToken,
        { ...demoPrivateProfile, storage: "local" },
      );
      result.data.consensus = false;
      if (!result.online) result.data.storage = "local";
      return result.data;
    },
    async reputation(address) {
      const passport = bundle.passports.find((row) => row.address === address) ?? null;
      const result = await read(
        baseUrl,
        `/reputation?address=${encodeURIComponent(address)}`,
        fetchImpl,
        options.sessionToken ?? null,
        passport?.reputations ?? null,
      );
      return view(result.online, result.data);
    },
    async missions() {
      const result = await read(baseUrl, "/missions", fetchImpl, null, bundle.missions);
      return view(result.online, result.data);
    },
    async grants() {
      const result = await read(baseUrl, "/grants", fetchImpl, null, bundle.grants);
      return view(result.online, result.data);
    },
    async bounties() {
      const result = await read(baseUrl, "/bounties", fetchImpl, null, bundle.bounties);
      return view(result.online, result.data);
    },
    async academy() {
      const result = await read(baseUrl, "/academy", fetchImpl, null, bundle.academy);
      return view(result.online, result.data);
    },
    async events() {
      const result = await read(baseUrl, "/events", fetchImpl, null, bundle.events);
      return view(result.online, result.data);
    },
    async merchants() {
      const result = await read(baseUrl, "/merchants", fetchImpl, null, bundle.merchants);
      return view(result.online, result.data);
    },
    async guilds() {
      const result = await read(baseUrl, "/guilds", fetchImpl, null, bundle.guilds);
      return view(result.online, result.data);
    },
    async proposals() {
      const result = await read(baseUrl, "/proposals", fetchImpl, null, bundle.proposals);
      return view(result.online, result.data);
    },
    async treasury() {
      const result = await read(baseUrl, "/treasury", fetchImpl, null, bundle.treasuries);
      return view(result.online, result.data);
    },
    async contributions() {
      const result = await read(baseUrl, "/contributions", fetchImpl, null, bundle.contributions);
      return view(result.online, result.data);
    },
    async hubs() {
      const result = await read(baseUrl, "/hubs", fetchImpl, null, bundle.hubs);
      return view(result.online, result.data);
    },
    async forum() {
      const result = await read(baseUrl, "/forum", fetchImpl, null, bundle.forum);
      return view(result.online, result.data);
    },
    async developers() {
      const result = await read(baseUrl, "/developers", fetchImpl, null, bundle.developers);
      return view(result.online, result.data);
    },
    async ecosystem() {
      const result = await read(baseUrl, "/ecosystem", fetchImpl, null, bundle.ecosystem);
      return view(result.online, result.data);
    },
  };
}

export function preferChainTreasuries(
  community: CommunityTreasuryRow[],
  chain:
    | {
        consensus_mutations_active?: boolean;
        treasuries?: Array<{ id: string; asset: "TLT" | "OVL" | "DRC"; balance: { base_units?: string } | string }>;
      }
    | null,
): CommunityTreasuryRow[] {
  if (!chain?.treasuries?.length) return community;
  return chain.treasuries.map((row) => {
    const balance =
      typeof row.balance === "string" ? row.balance : (row.balance.base_units ?? "0");
    return {
      id: row.id,
      asset: row.asset,
      balance,
      source: "indexed",
      note: chain.consensus_mutations_active
        ? "Node-reported canonical treasury. This light client did not prove the balance against a header."
        : "Node-reported canonical treasury scaffold. Spend mutations are not active. Balance is not a light-client proof.",
    };
  });
}
