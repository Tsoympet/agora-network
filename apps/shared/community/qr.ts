/**
 * Versioned Agora QR payloads.
 * Ambiguous or partial parses fail closed. Payment kinds always expose
 * destination and amount so a wallet can show both before any signature.
 */

export const QR_VERSION = 1 as const;

export const QR_KINDS = [
  "drc-payment",
  "merchant-pay",
  "address",
  "passport-share",
  "event",
  "mission",
  "grant",
  "profile",
] as const;

export type QrKind = (typeof QR_KINDS)[number];

export type AgoraQrPayload = {
  version: typeof QR_VERSION;
  kind: QrKind;
  fields: Record<string, string>;
};

export type QrPreview = {
  kind: QrKind;
  destination: string | null;
  amount: string | null;
  asset: "DRC" | null;
  requiresSignature: boolean;
  summary: string;
};

export type QrParseResult =
  | { ok: true; payload: AgoraQrPayload; preview: QrPreview }
  | { ok: false; error: string };

const PAYMENT_KINDS = new Set<QrKind>(["drc-payment", "merchant-pay"]);

const ALLOWED: Record<QrKind, readonly string[]> = {
  "drc-payment": ["to", "amount", "memo", "invoice"],
  "merchant-pay": ["to", "amount", "memo", "invoice", "merchant"],
  address: ["addr"],
  "passport-share": ["address", "username"],
  event: ["id"],
  mission: ["id"],
  grant: ["id"],
  profile: ["address"],
};

function fail(error: string): QrParseResult {
  return { ok: false, error };
}

function isKind(value: string): value is QrKind {
  return (QR_KINDS as readonly string[]).includes(value);
}

function positiveInteger(value: string): boolean {
  return /^[1-9][0-9]*$/.test(value);
}

function previewFor(payload: AgoraQrPayload): QrPreview | string {
  const { kind, fields } = payload;
  if (PAYMENT_KINDS.has(kind)) {
    const destination = fields.to;
    const amount = fields.amount;
    if (!destination || !amount) return "payment is missing destination or amount";
    if (!positiveInteger(amount)) return "payment amount must be a positive integer";
    return {
      kind,
      destination,
      amount,
      asset: "DRC",
      requiresSignature: true,
      summary: `DRC ${amount} to ${destination}`,
    };
  }
  if (kind === "address") {
    if (!fields.addr) return "address payload is missing addr";
    return {
      kind,
      destination: fields.addr,
      amount: null,
      asset: null,
      requiresSignature: false,
      summary: `Address ${fields.addr}`,
    };
  }
  const id = fields.id ?? fields.address ?? fields.username;
  if (!id) return `${kind} payload is missing its identifier`;
  return {
    kind,
    destination: fields.address ?? null,
    amount: null,
    asset: null,
    requiresSignature: false,
    summary: `${kind} ${id}`,
  };
}

function accept(kind: QrKind, fields: Record<string, string>): QrParseResult {
  const allowed = new Set(ALLOWED[kind]);
  for (const key of Object.keys(fields)) {
    if (!allowed.has(key)) return fail(`unexpected field ${key}`);
    if (!fields[key]) return fail(`empty field ${key}`);
  }
  if (PAYMENT_KINDS.has(kind)) {
    if (fields.asset) return fail("DRC payment payloads cannot retarget the asset");
  }
  const payload: AgoraQrPayload = { version: QR_VERSION, kind, fields };
  const preview = previewFor(payload);
  if (typeof preview === "string") return fail(preview);
  return { ok: true, payload, preview };
}

function parseQuery(query: string): Record<string, string> | string {
  if (!query) return "missing query";
  const fields: Record<string, string> = {};
  for (const part of query.split("&")) {
    if (!part || !part.includes("=")) return "malformed query";
    const eq = part.indexOf("=");
    const key = decodeURIComponent(part.slice(0, eq));
    const value = decodeURIComponent(part.slice(eq + 1));
    if (key in fields) return `duplicate field ${key}`;
    fields[key] = value;
  }
  return fields;
}

function parseUri(input: string): QrParseResult {
  if (input.includes("#") || input.includes(" ")) return fail("ambiguous payload");
  const match = /^agora:([0-9]+):([a-z-]+)\?([^]*)$/.exec(input);
  if (!match) return fail("unrecognized Agora QR");
  const version = Number(match[1]);
  if (version !== QR_VERSION) return fail(`unsupported QR version ${match[1]}`);
  const kindRaw = match[2];
  if (!kindRaw || !isKind(kindRaw)) return fail("unknown QR kind");
  const fields = parseQuery(match[3] ?? "");
  if (typeof fields === "string") return fail(fields);
  return accept(kindRaw, fields);
}

function parseJson(input: string): QrParseResult {
  let value: unknown;
  try {
    value = JSON.parse(input);
  } catch {
    return fail("invalid JSON QR");
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return fail("QR JSON must be an object");
  }
  const rec = value as Record<string, unknown>;
  if (typeof rec.agoraQr !== "number" || typeof rec.kind !== "string") {
    return fail("QR JSON missing agoraQr version or kind");
  }
  if (rec.agoraQr !== QR_VERSION) return fail(`unsupported QR version ${String(rec.agoraQr)}`);
  if (!isKind(rec.kind)) return fail("unknown QR kind");
  const fields: Record<string, string> = {};
  for (const [key, field] of Object.entries(rec)) {
    if (key === "agoraQr" || key === "kind") continue;
    if (typeof field !== "string") return fail(`field ${key} must be a string`);
    fields[key] = field;
  }
  return accept(rec.kind, fields);
}

/** Fail closed. A bare address or a mixed encoding is not a payment. */
export function parseAgoraQr(input: string): QrParseResult {
  const text = input.trim();
  if (!text) return fail("empty QR");
  const looksJson = text.startsWith("{");
  const looksUri = text.startsWith("agora:");
  if (looksJson && looksUri) return fail("ambiguous payload");
  if (looksJson) return parseJson(text);
  if (looksUri) return parseUri(text);
  return fail("unrecognized Agora QR");
}

export function encodeAgoraQr(payload: AgoraQrPayload): string {
  const parsed = accept(payload.kind, payload.fields);
  if (!parsed.ok) throw new Error(parsed.error);
  const query = Object.entries(payload.fields)
    .map(([key, value]) => `${encodeURIComponent(key)}=${encodeURIComponent(value)}`)
    .join("&");
  return `agora:${QR_VERSION}:${payload.kind}?${query}`;
}

/** Wallets call this before asking for a signature. */
export function assertPaymentPreview(preview: QrPreview): void {
  if (!PAYMENT_KINDS.has(preview.kind)) return;
  if (!preview.destination || !preview.amount || preview.asset !== "DRC") {
    throw new Error("refusing to sign without destination and amount");
  }
}
