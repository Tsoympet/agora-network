/**
 * JSON file for the community infrastructure process.
 * This is infrastructure trust: the operator can edit the file. It is not
 * consensus state, not a header proof, and not a treasury balance.
 */
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

import { communityFixtures } from "../../apps/shared/community/fixtures.ts";
import type { CommunityBundle, ForumPost, Mission } from "../../apps/shared/community/types.ts";

export const COMMUNITY_STORE_TRUST =
  "Infrastructure file. The operator who runs this process can read, edit, and delete it. Rows are not consensus, not a header proof, and not a treasury balance.";

export type MissionReview = { missionId: string; note: string };

export type CommunityStoreData = {
  schemaVersion: 1;
  trust: typeof COMMUNITY_STORE_TRUST;
  bundle: CommunityBundle;
  reviews: MissionReview[];
};

function isStore(value: unknown): value is CommunityStoreData {
  if (!value || typeof value !== "object") return false;
  const row = value as Partial<CommunityStoreData>;
  return row.schemaVersion === 1 && !!row.bundle && Array.isArray(row.bundle.forum) && Array.isArray(row.reviews);
}

function seedStore(): CommunityStoreData {
  return {
    schemaVersion: 1,
    trust: COMMUNITY_STORE_TRUST,
    bundle: communityFixtures(),
    reviews: [],
  };
}

export function openCommunityStore(path: string) {
  let data: CommunityStoreData;
  let seeded = false;
  try {
    const parsed = JSON.parse(readFileSync(path, "utf8")) as unknown;
    if (!isStore(parsed)) throw new Error("community store schema mismatch");
    parsed.trust = COMMUNITY_STORE_TRUST;
    data = parsed;
  } catch (err) {
    const code = err && typeof err === "object" && "code" in err ? (err as { code?: string }).code : undefined;
    if (code === "ENOENT") {
      data = seedStore();
      seeded = true;
    } else if (err instanceof SyntaxError) {
      throw new Error("community store is not valid JSON");
    } else {
      throw err;
    }
  }

  function save() {
    data.trust = COMMUNITY_STORE_TRUST;
    data.schemaVersion = 1;
    mkdirSync(dirname(path), { recursive: true });
    const tmp = `${path}.tmp`;
    writeFileSync(tmp, `${JSON.stringify(data)}\n`);
    renameSync(tmp, path);
  }

  if (seeded) save();

  return {
    path,
    trust: COMMUNITY_STORE_TRUST,
    data,
    save,
    replaceForum(posts: ForumPost[]) {
      data.bundle.forum = posts;
      save();
    },
    replaceMissions(missions: Mission[], reviews: MissionReview[]) {
      data.bundle.missions = missions;
      data.reviews = reviews;
      save();
    },
  };
}

export type CommunityStore = ReturnType<typeof openCommunityStore>;
