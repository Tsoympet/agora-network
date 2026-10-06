/** Device-local public passport export. Private keys are refused. */

import { TRUST_ASSUMPTIONS, TRUST_ASSUMPTION_IDS } from "./trustAssumptions.ts";

export const PORTABLE_IDENTITY_FORMAT = "agora-portable-identity-v1";
export const PUBLIC_PASSPORT_VERSION = "agora-public-passport-v1";
export const LOCAL_PREFS_VERSION = "agora-local-prefs-v1";
export const PASSPORT_EXPORT_NOTE =
  "Public contribution evidence. This export is not a personhood check and it is not Sybil resistance.";

const SECRET_KEYS = [
  "mnemonic",
  "private_key",
  "privateKey",
  "seed",
  "xprv",
  "secret",
  "vault_password",
  "wallet_password",
  "privkey",
];

export type PublicAttestationView = {
  category: string;
  issuer_label: string;
  evidence_hash_hex: string;
  service_trust_class: "hub_signed_unchecked";
  service_trust_assumption: string;
};

export type ServiceTrustLabel = {
  service: string;
  class: string;
  assumption: string;
};

export type PortableIdentityBundle = {
  format: typeof PORTABLE_IDENTITY_FORMAT;
  contains_private_keys: false;
  public_passport: {
    version: typeof PUBLIC_PASSPORT_VERSION;
    subject_address: string | null;
    attestations: PublicAttestationView[];
    note: string;
  };
  local_private_prefs: {
    version: typeof LOCAL_PREFS_VERSION;
    language: string | null;
    region_query: string | null;
    interest_filters: string[];
  };
  service_trust: ServiceTrustLabel[];
};

export type IdentityExportInput = {
  subject_address?: string | null;
  attestations?: Array<{
    category: string;
    issuer_label: string;
    evidence_hash_hex: string;
  }>;
  language?: string | null;
  region_query?: string | null;
  interest_filters?: string[];
};

export function exportPortableIdentity(input: IdentityExportInput): string {
  return JSON.stringify(buildPortableIdentity(input), null, 2);
}

export function parsePortableIdentity(raw: string): PortableIdentityBundle {
  const value: unknown = JSON.parse(raw);
  rejectSecretKeys(value);
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("identity bundle must be an object");
  }
  const bundle = value as PortableIdentityBundle;
  if (bundle.format !== PORTABLE_IDENTITY_FORMAT) throw new Error("unknown identity format");
  if (bundle.contains_private_keys !== false) throw new Error("private keys are refused");
  if (bundle.public_passport?.version !== PUBLIC_PASSPORT_VERSION) {
    throw new Error("unknown passport version");
  }
  if (!bundle.public_passport.note?.trim()) throw new Error("passport note is required");
  if (bundle.local_private_prefs?.version !== LOCAL_PREFS_VERSION) {
    throw new Error("unknown prefs version");
  }
  if (!Array.isArray(bundle.service_trust) || bundle.service_trust.length !== TRUST_ASSUMPTION_IDS.length) {
    throw new Error("service trust labels are incomplete");
  }
  TRUST_ASSUMPTION_IDS.forEach((id, index) => {
    const label = bundle.service_trust[index];
    if (!label || label.service !== id || !label.class?.trim() || !label.assumption?.trim()) {
      throw new Error("service trust label is incomplete");
    }
  });
  return bundle;
}

function buildPortableIdentity(input: IdentityExportInput): PortableIdentityBundle {
  const attestations = (input.attestations ?? []).map((attestation) => {
    const category = attestation.category.trim();
    const issuer = attestation.issuer_label.trim();
    const hash = attestation.evidence_hash_hex.trim().toLowerCase();
    if (!category || !issuer || [...category].length > 80 || [...issuer].length > 80) {
      throw new Error("attestation label length");
    }
    rejectSecretText("attestation", category);
    rejectSecretText("attestation", issuer);
    if (!/^[0-9a-f]{64}$/.test(hash)) {
      throw new Error("evidence hash must be 64 hex characters");
    }
    return {
      category,
      issuer_label: issuer,
      evidence_hash_hex: hash,
      service_trust_class: "hub_signed_unchecked" as const,
      service_trust_assumption: "Issuer signatures are not re-checked in this export.",
    };
  });
  return {
    format: PORTABLE_IDENTITY_FORMAT,
    contains_private_keys: false,
    public_passport: {
      version: PUBLIC_PASSPORT_VERSION,
      subject_address: cleanSubject(input.subject_address),
      attestations,
      note: PASSPORT_EXPORT_NOTE,
    },
    local_private_prefs: {
      version: LOCAL_PREFS_VERSION,
      language: cleanShort(input.language, 32, "language"),
      region_query: cleanShort(input.region_query, 80, "region query"),
      interest_filters: (input.interest_filters ?? []).map((interest) => {
        const trimmed = interest.trim();
        if (!trimmed || [...trimmed].length > 64) throw new Error("interest filter length");
        rejectSecretText("interest", trimmed);
        return trimmed;
      }),
    },
    service_trust: TRUST_ASSUMPTIONS.map((row) => ({
      service: row.id,
      class: row.posture,
      assumption: row.assumption,
    })),
  };
}

function cleanSubject(value: string | null | undefined): string | null {
  if (!value?.trim()) return null;
  const trimmed = value.trim();
  rejectSecretText("subject", trimmed);
  if (/\s/.test(trimmed) || [...trimmed].length > 128) {
    throw new Error("subject must be one public address");
  }
  return trimmed;
}

function cleanShort(value: string | null | undefined, max: number, field: string): string | null {
  if (!value?.trim()) return null;
  const trimmed = value.trim();
  rejectSecretText(field, trimmed);
  if ([...trimmed].length > max) throw new Error(`${field} is too long`);
  return trimmed;
}

function rejectSecretText(field: string, text: string): void {
  const words = text.trim().split(/\s+/);
  const lower = text.toLowerCase();
  if (
    words.length >= 12 ||
    ["mnemonic", "xprv", "private_key", "private key", "seed phrase"].some((marker) =>
      lower.includes(marker),
    )
  ) {
    throw new Error(`${field} looks like key material and was refused`);
  }
}

function rejectSecretKeys(value: unknown): void {
  if (Array.isArray(value)) {
    for (const item of value) rejectSecretKeys(item);
    return;
  }
  if (!value || typeof value !== "object") return;
  for (const [key, child] of Object.entries(value)) {
    if (SECRET_KEYS.includes(key)) throw new Error(`secret field ${key}`);
    rejectSecretKeys(child);
  }
}
