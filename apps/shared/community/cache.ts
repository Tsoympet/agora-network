import type { DataPlane } from "../data-plane/planes.ts";
import type {
  AcademyCatalog,
  AcademyProgress,
  CachedDoc,
  NotificationPrefs,
  PrivatePassportProfile,
  PublicPassport,
} from "./types.ts";

export const COMMUNITY_CACHE_KEY = "agora-community-cache-v1";

export type CommunityCacheBlob = {
  schemaVersion: 1;
  savedAt: number;
  passport: PublicPassport | null;
  privateProfile: PrivatePassportProfile | null;
  academy: AcademyCatalog | null;
  progress: AcademyProgress[];
  docs: CachedDoc[];
  notifications: NotificationPrefs;
};

export type CacheStorage = {
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<void>;
};

export type OfflineView<T> = {
  data: T | null;
  online: boolean;
  fromCache: boolean;
  confirmed: false;
  label: string;
  /** Which plane produced `data`. */
  plane: DataPlane;
};

export function defaultNotificationPrefs(): NotificationPrefs {
  return {
    schemaVersion: 1,
    missions: true,
    assembly: true,
    grants: false,
    merchants: true,
    includeAmounts: false,
  };
}

export function emptyCache(now: number): CommunityCacheBlob {
  return {
    schemaVersion: 1,
    savedAt: now,
    passport: null,
    privateProfile: null,
    academy: null,
    progress: [],
    docs: [],
    notifications: defaultNotificationPrefs(),
  };
}

export function memoryCacheStorage(): CacheStorage & { snapshot(): string | null } {
  let value: string | null = null;
  return {
    async get() {
      return value;
    },
    async set(_key, next) {
      value = next;
    },
    snapshot() {
      return value;
    },
  };
}

export async function loadCache(storage: CacheStorage, now: number): Promise<CommunityCacheBlob> {
  const raw = await storage.get(COMMUNITY_CACHE_KEY);
  if (!raw) return emptyCache(now);
  const parsed = JSON.parse(raw) as CommunityCacheBlob;
  if (parsed.schemaVersion !== 1) return emptyCache(now);
  if (parsed.privateProfile && parsed.privateProfile.consensus !== false) {
    throw new Error("cached private profile was marked consensus");
  }
  return parsed;
}

export async function saveCache(storage: CacheStorage, blob: CommunityCacheBlob): Promise<void> {
  if (blob.privateProfile) blob.privateProfile.consensus = false;
  blob.notifications.includeAmounts = false;
  await storage.set(COMMUNITY_CACHE_KEY, JSON.stringify(blob));
}

export function presentCached<T>(
  online: boolean,
  data: T | null,
  fromCache: boolean,
  plane: DataPlane,
): OfflineView<T> {
  return {
    data,
    online,
    fromCache,
    confirmed: false,
    plane,
    label: online
      ? "live community read · not a consensus confirmation"
      : "cached view · not confirmed",
  };
}

export function notificationBody(title: string, detail: string): { title: string; body: string } {
  const scrubbed = detail.replace(/\b\d[\d,]*\s*(DRC|TLT|OVL|base units)\b/gi, "[amount hidden]");
  if (/\b\d[\d,]*\s*(DRC|TLT|OVL)\b/i.test(scrubbed)) {
    throw new Error("notification still contains an amount");
  }
  return { title, body: scrubbed };
}

export function recordLessonProgress(
  progress: AcademyProgress[],
  courseId: string,
  lessonId: string,
): AcademyProgress[] {
  const next = progress.map((row) => ({ ...row, completedLessonIds: [...row.completedLessonIds] }));
  const row = next.find((item) => item.courseId === courseId);
  if (!row) {
    next.push({
      schemaVersion: 1,
      courseId,
      completedLessonIds: [lessonId],
      storage: "local",
    });
    return next;
  }
  if (!row.completedLessonIds.includes(lessonId)) row.completedLessonIds.push(lessonId);
  return next;
}
