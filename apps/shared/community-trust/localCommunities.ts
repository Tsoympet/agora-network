/** Device-local community attributes. Country is free text, not an enum. */

export const MANUAL_REGION_SEARCH_NOTICE =
  "Type a country, region, city, language, interest, or specialization. Search uses that text on this device. GPS is not requested.";

const FORBIDDEN_KEYS = [
  "latitude",
  "longitude",
  "lat",
  "lon",
  "lng",
  "gps",
  "coordinates",
  "geolocation",
  "altitude",
] as const;

export type LocalCommunity = {
  id: string;
  name: string;
  country?: string;
  region?: string;
  city?: string;
  languages: string[];
  interests: string[];
  specializations: string[];
};

export function requestsDeviceLocation(): false {
  return false;
}

export function seedLocalCommunities(): LocalCommunity[] {
  return [
    {
      id: "greece-attica-athens",
      name: "Athens Agora",
      country: "Greece",
      region: "Attica",
      city: "Athens",
      languages: ["el", "en"],
      interests: ["governance", "civic assembly"],
      specializations: ["constitution", "public square"],
    },
    {
      id: "europe",
      name: "Europe",
      region: "Europe",
      languages: ["en", "el", "de", "fr"],
      interests: ["payments", "merchants"],
      specializations: ["DRC payments"],
    },
    {
      id: "asia",
      name: "Asia",
      region: "Asia",
      languages: ["en", "zh", "ja", "ko"],
      interests: ["developers", "academy"],
      specializations: ["protocol engineering"],
    },
    {
      id: "americas",
      name: "Americas",
      region: "Americas",
      languages: ["en", "es", "pt"],
      interests: ["miners", "nodes"],
      specializations: ["TLT infrastructure"],
    },
  ];
}

export function localCommunityFromJson(raw: unknown): LocalCommunity {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
    throw new Error("local community must be an object");
  }
  const record = raw as Record<string, unknown>;
  for (const key of Object.keys(record)) {
    if ((FORBIDDEN_KEYS as readonly string[]).includes(key)) {
      throw new Error(`field ${key} is not a community attribute`);
    }
  }
  const id = requiredLabel(record.id, 64, "id");
  if (!/^[a-z0-9][a-z0-9-]{0,63}$/.test(id)) {
    throw new Error("id must be a lowercase slug");
  }
  const community: LocalCommunity = {
    id,
    name: requiredLabel(record.name, 80, "name"),
    languages: labelList(record.languages, "language"),
    interests: labelList(record.interests, "interest"),
    specializations: labelList(record.specializations, "specialization"),
  };
  const country = optionalLabel(record.country, 80, "country");
  const region = optionalLabel(record.region, 80, "region");
  const city = optionalLabel(record.city, 80, "city");
  if (country) community.country = country;
  if (region) community.region = region;
  if (city) community.city = city;
  if (
    !community.country &&
    !community.region &&
    !community.city &&
    community.languages.length === 0 &&
    community.interests.length === 0 &&
    community.specializations.length === 0
  ) {
    throw new Error(
      "a community needs a country, region, city, language, interest, or specialization",
    );
  }
  return community;
}

export function searchLocalCommunities(
  records: readonly LocalCommunity[],
  query: string,
): LocalCommunity[] {
  const trimmed = query.trim().toLowerCase();
  if (!trimmed) return [...records];
  const tokens = trimmed.split(/\s+/);
  return records.filter((record) => {
    const hay = haystack(record);
    return tokens.every((token) => hay.includes(token));
  });
}

function haystack(record: LocalCommunity): string {
  return [
    record.id,
    record.name,
    record.country ?? "",
    record.region ?? "",
    record.city ?? "",
    ...record.languages,
    ...record.interests,
    ...record.specializations,
  ]
    .join(" ")
    .toLowerCase();
}

function requiredLabel(value: unknown, max: number, field: string): string {
  if (typeof value !== "string") throw new Error(`${field} length`);
  const trimmed = value.trim();
  if (!trimmed || [...trimmed].length > max) throw new Error(`${field} length`);
  return trimmed;
}

function optionalLabel(value: unknown, max: number, field: string): string | undefined {
  if (value == null) return undefined;
  return requiredLabel(value, max, field);
}

function labelList(value: unknown, field: string): string[] {
  if (value == null) return [];
  if (!Array.isArray(value) || value.length > 16) {
    throw new Error(`too many ${field} values`);
  }
  return value.map((item) => requiredLabel(item, 64, field));
}
