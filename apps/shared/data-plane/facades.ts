/**
 * HTTP clients for infrastructure. Device packages import this module and do not
 * start the services. A missing base URL fails closed instead of booting a server
 * or inventing a balance.
 */

export type InfrastructureFetch = (
  input: string,
  init?: { method?: string; headers?: Record<string, string>; body?: string },
) => Promise<{
  ok: boolean;
  status?: number;
  json: () => Promise<unknown>;
}>;

export class InfrastructureUnconfigured extends Error {
  readonly service: string;

  constructor(service: string) {
    super(`${service} is not configured. Device packages do not start an in-process server.`);
    this.name = "InfrastructureUnconfigured";
    this.service = service;
  }
}

export class InfrastructureResponseError extends Error {
  readonly service: string;
  readonly status: number;

  constructor(service: string, status: number, detail: string) {
    super(`${service} HTTP ${status}: ${detail}`);
    this.name = "InfrastructureResponseError";
    this.service = service;
    this.status = status;
  }
}

type Envelope<T> = {
  plane: "infrastructure";
  service: string;
  trust: string;
  chainProof: false;
  data: T;
  error?: string;
};

function isEnvelope(body: unknown): body is Envelope<unknown> {
  if (!body || typeof body !== "object") return false;
  const row = body as Partial<Envelope<unknown>>;
  return row.plane === "infrastructure" && row.chainProof === false && typeof row.service === "string";
}

export function createInfrastructureFacades(options: {
  baseUrl?: string | null;
  fetchImpl?: InfrastructureFetch;
}) {
  const baseUrl = options.baseUrl?.replace(/\/$/, "") ?? "";
  const fetchImpl = options.fetchImpl ?? (globalThis.fetch as InfrastructureFetch | undefined);

  async function request<T>(
    service: string,
    path: string,
    init?: { method?: string; headers?: Record<string, string>; body?: string },
  ): Promise<T> {
    if (!baseUrl) throw new InfrastructureUnconfigured(service);
    if (!fetchImpl) throw new InfrastructureUnconfigured(service);
    const response = await fetchImpl(`${baseUrl}${path}`, init);
    const body = await response.json();
    if (!isEnvelope(body) || body.service !== service) {
      throw new Error(`${service} response must stay an infrastructure record and must not claim a chain proof`);
    }
    if ("chainData" in (body as object) && (body as { chainData?: unknown }).chainData != null) {
      throw new Error(`${service} attached chain data to an infrastructure envelope`);
    }
    const status = typeof response.status === "number" ? response.status : response.ok ? 200 : 500;
    if (!response.ok) {
      throw new InfrastructureResponseError(service, status, body.error ?? "unavailable");
    }
    return body.data as T;
  }

  function get<T>(service: string, path: string, sessionToken?: string | null): Promise<T> {
    const headers: Record<string, string> = {};
    if (sessionToken) headers.authorization = `Bearer ${sessionToken}`;
    return request<T>(service, path, { headers });
  }

  function post<T>(service: string, path: string, body: unknown): Promise<T> {
    return request<T>(service, path, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
  }

  return {
    configured: baseUrl.length > 0,
    get,
    post,
    forum: {
      list: () => get<unknown[]>("forum", "/forum"),
      reply: (body: { postId: string; body: string; authorAddress: string; authorUsername?: string }) =>
        post<{ id: string; body: string; source: string }>("forum", "/forum/replies", body),
    },
    grants: {
      list: () => get<unknown[]>("grant-admin", "/grants"),
      missions: () => get<unknown[]>("grant-admin", "/missions"),
      advance: (body: { id: string; to: string }) =>
        post<{ recorded: boolean }>("grant-admin", "/missions/advance", body),
    },
    events: {
      list: () => get<unknown[]>("event-service", "/events"),
    },
    search: {
      query: (q: string) => get<unknown[]>("search", `/search?q=${encodeURIComponent(q)}`),
    },
    moderation: {
      report: (body: { postId: string; reason: string }) =>
        post<{ accepted: boolean }>("moderation", "/moderation/reports", body),
    },
    notifications: {
      register: (token: string) =>
        post<{ registered: boolean }>("notification-dispatch", "/notifications/register", { token }),
    },
    indexer: {
      status: () => get<{ upstream: boolean; chainData: null }>("indexer", "/v1/indexer/status"),
    },
  };
}

export type InfrastructureFacades = ReturnType<typeof createInfrastructureFacades>;
