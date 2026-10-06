/**
 * Versioned Agora community service types shared by the PC light client,
 * the phone light client, and the local community API.
 *
 * Passport is a non-transferable identity credential. It is not a financial
 * token. Wallet keys, community sessions, and private profile fields stay
 * separate. Consensus mutations are not implied by these records.
 */

export const COMMUNITY_SCHEMA_VERSION = 1 as const;

export const DATA_SOURCES = [
  "on-chain verified",
  "cryptographically verified",
  "indexed",
  "community submitted",
  "external",
] as const;

export type DataSource = (typeof DATA_SOURCES)[number];

export const REPUTATION_CATEGORIES = [
  "Builder",
  "Community",
  "Merchant",
  "Security",
  "Mining",
  "Governance",
  "Education",
] as const;

export type ReputationCategory = (typeof REPUTATION_CATEGORIES)[number];

export type ReputationScores = Record<ReputationCategory, number>;

export type Badge = {
  id: string;
  name: string;
  category: ReputationCategory;
  /** Passport badges cannot move between addresses. */
  nonTransferable: true;
  evidenceId: string;
  source: DataSource;
};

export type PublicPassport = {
  schemaVersion: typeof COMMUNITY_SCHEMA_VERSION;
  address: string;
  username: string;
  avatarHash: string | null;
  roles: string[];
  reputations: ReputationScores;
  overall: number;
  badges: Badge[];
  contributionCounts: Record<string, number>;
  source: DataSource;
};

/** Email and language never enter consensus state. */
export type PrivatePassportProfile = {
  schemaVersion: typeof COMMUNITY_SCHEMA_VERSION;
  address: string;
  email: string | null;
  language: string;
  storage: "local" | "authenticated-service";
  consensus: false;
};

export const MISSION_STATES = [
  "AVAILABLE",
  "ACCEPTED",
  "IN_PROGRESS",
  "SUBMITTED",
  "COMPLETED",
] as const;

export type MissionState = (typeof MISSION_STATES)[number];

export type Mission = {
  id: string;
  title: string;
  summary: string;
  state: MissionState;
  category: ReputationCategory;
  rewardNote: string;
  source: DataSource;
};

export type AcademyLesson = {
  id: string;
  title: string;
  source: DataSource;
};

export type AcademyCourse = {
  id: string;
  track: string;
  title: string;
  lessons: AcademyLesson[];
  source: DataSource;
};

export type AcademyCatalog = {
  schemaVersion: typeof COMMUNITY_SCHEMA_VERSION;
  courses: AcademyCourse[];
};

/** Lesson completion stays on the device until an academy cert event exists. */
export type AcademyProgress = {
  schemaVersion: typeof COMMUNITY_SCHEMA_VERSION;
  courseId: string;
  completedLessonIds: string[];
  storage: "local";
};

export type GrantMilestone = {
  id: string;
  title: string;
  evidenceHash: string | null;
  accepted: boolean;
};

export type Grant = {
  id: string;
  title: string;
  asset: "TLT" | "OVL" | "DRC";
  beneficiary: string;
  total: string;
  milestones: GrantMilestone[];
  source: DataSource;
  /** Grants do not move treasury funds from this client. */
  disbursesFunds: false;
};

export type Bounty = {
  id: string;
  title: string;
  category: ReputationCategory;
  status: "open" | "claimed" | "paid";
  source: DataSource;
};

export const GUILD_KINDS = [
  "Builder",
  "Node",
  "Miner",
  "Merchant",
  "Security",
  "Education",
  "Community",
] as const;

export type GuildKind = (typeof GUILD_KINDS)[number];

export type Guild = {
  id: string;
  kind: GuildKind;
  charter: string;
  memberCount: number;
  source: DataSource;
};

export type MerchantProfile = {
  id: string;
  displayName: string;
  /** Receiving address only. Merchant spending keys are never stored. */
  drcAddress: string;
  region: string;
  category: string;
  invoiceMemo: string | null;
  source: DataSource;
  holdsMerchantKeys: false;
};

export type CommunityEvent = {
  id: string;
  title: string;
  region: string;
  startsAt: string;
  hubId: string | null;
  source: DataSource;
};

export type Hub = {
  id: string;
  name: string;
  region: string;
  kind: "geographic" | "specialist";
  charterHash: string | null;
  source: DataSource;
};

export type ForumReply = {
  id: string;
  authorAddress: string;
  authorUsername: string;
  body: string;
  source: DataSource;
};

export type ForumPost = {
  id: string;
  category: string;
  authorAddress: string;
  authorUsername: string;
  title: string;
  body: string;
  replies: ForumReply[];
  reportCount: number;
  source: DataSource;
};

export type ForumReport = {
  postId: string;
  reporterAddress: string;
  reason: string;
  source: "community submitted";
};

export const GOVERNANCE_AREA_IDS = [
  "technical",
  "drc",
  "ovl",
  "tlt",
  "community",
  "treasury",
  "education",
  "merchant",
] as const;

export type GovernanceAreaId = (typeof GOVERNANCE_AREA_IDS)[number];

export type GovernanceArea = {
  id: GovernanceAreaId;
  label: string;
  eligibility: string;
};

export type ProposalBinding = "on-chain" | "advisory";

export type CommunityProposal = {
  id: string;
  area: GovernanceAreaId;
  title: string;
  summary: string;
  binding: ProposalBinding;
  /** Present only when a full node committed this act. Absent means advisory. */
  chainCommitment: string | null;
  source: DataSource;
};

export type CommunityTreasuryRow = {
  id: string;
  asset: "TLT" | "OVL" | "DRC";
  balance: string;
  source: DataSource;
  note: string;
};

export type Contribution = {
  id: string;
  kind: "mission_complete" | "grant_milestone" | "vote_recorded" | "academy_cert";
  title: string;
  at: string;
  source: DataSource;
};

export type DirectoryEntry = {
  id: string;
  name: string;
  address: string;
  focus: string;
  source: DataSource;
};

export type NotificationPrefs = {
  schemaVersion: typeof COMMUNITY_SCHEMA_VERSION;
  missions: boolean;
  assembly: boolean;
  grants: boolean;
  merchants: boolean;
  /** Push copies must not include amounts or balances. */
  includeAmounts: false;
};

export type CachedDoc = {
  id: string;
  title: string;
  body: string;
  source: DataSource;
};

export type EcosystemSnapshot = {
  schemaVersion: typeof COMMUNITY_SCHEMA_VERSION;
  passportCount: number;
  missionCount: number;
  grantCount: number;
  merchantCount: number;
  eventCount: number;
  source: DataSource;
};

export type CommunityBundle = {
  schemaVersion: typeof COMMUNITY_SCHEMA_VERSION;
  maturity: "Experimental";
  passports: PublicPassport[];
  missions: Mission[];
  academy: AcademyCatalog;
  grants: Grant[];
  bounties: Bounty[];
  guilds: Guild[];
  merchants: MerchantProfile[];
  events: CommunityEvent[];
  hubs: Hub[];
  forum: ForumPost[];
  proposals: CommunityProposal[];
  treasuries: CommunityTreasuryRow[];
  contributions: Contribution[];
  developers: DirectoryEntry[];
  docs: CachedDoc[];
  ecosystem: EcosystemSnapshot;
};
