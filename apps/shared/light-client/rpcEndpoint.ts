/**
 * User-chosen JSON-RPC endpoint for a light client.
 *
 * The URL is a node the device trusts for node-reported balances. It is not an
 * Agora account. Loopback is valid on the machine that runs the node and is
 * omitted from device-to-device pairing payloads.
 */
import type { VaultStorage } from "./vault";

export const RPC_ENDPOINT_STORAGE_KEY = "agora.light.rpc.v1";
export const RPC_TOKEN_STORAGE_KEY = "agora.light.rpc.token.v1";

const MAX_URL_CHARS = 512;
const MAX_TOKEN_CHARS = 256;

export type RpcReach = "loopback" | "lan" | "public";

export function validateRpcUrl(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed || trimmed.length > MAX_URL_CHARS) {
    throw new Error("RPC URL must be http or https");
  }
  if (/\s/.test(trimmed)) {
    throw new Error("RPC URL must be http or https");
  }
  let url: URL;
  try {
    url = new URL(trimmed);
  } catch {
    throw new Error("RPC URL must be http or https");
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("RPC URL must be http or https");
  }
  if (url.username || url.password) {
    throw new Error("RPC URL must not embed credentials");
  }
  if (!url.hostname) {
    throw new Error("RPC URL must be http or https");
  }
  return url.toString();
}

function ipv4Parts(host: string): number[] | null {
  const parts = host.split(".");
  if (parts.length !== 4) return null;
  const nums = parts.map((part) => Number(part));
  if (nums.some((n) => !Number.isInteger(n) || n < 0 || n > 255)) return null;
  return nums;
}

/** Classify where a saved endpoint can be reached from another device. */
export function classifyRpcReach(raw: string): RpcReach {
  const url = new URL(validateRpcUrl(raw));
  const host = url.hostname.toLowerCase().replace(/^\[|\]$/g, "");
  if (
    host === "localhost" ||
    host.endsWith(".localhost") ||
    host === "::1" ||
    host === "0.0.0.0"
  ) {
    return "loopback";
  }
  const v4 = ipv4Parts(host);
  if (v4) {
    const [a, b] = v4;
    if (a === 127 || a === 0) return "loopback";
    if (a === 10 || a === 192 && b === 168 || a === 172 && b >= 16 && b <= 31) {
      return "lan";
    }
    if (a === 169 && b === 254) return "lan";
  }
  if (host.endsWith(".local")) return "lan";
  // Unique-local IPv6 (fc00::/7). A hostname that merely starts with "fd" is public.
  if (host.includes(":") && (host.startsWith("fc") || host.startsWith("fd") || host.startsWith("fe80:"))) {
    return "lan";
  }
  return "public";
}

/**
 * Honest trust copy for the endpoint the wallet is using.
 * Header-checked TLT inclusion is unaffected: that proof is recomputed locally.
 */
export function rpcTrustWarning(raw: string): string {
  const url = new URL(validateRpcUrl(raw));
  const reach = classifyRpcReach(raw);
  const parts: string[] = [];
  if (url.protocol === "http:") {
    parts.push(
      "This RPC URL is plain HTTP. A network attacker can read or alter what this device is shown.",
    );
  }
  if (reach === "loopback") {
    parts.push(
      "This endpoint is localhost. Another device cannot use it. Away from this machine, set a LAN or public http(s) RPC.",
    );
  } else if (reach === "lan") {
    parts.push(
      "This endpoint is on the local network. It works on the same LAN and does not follow you away from home.",
    );
  }
  parts.push(
    "Node-reported TLT, OVL, and DRC balances trust this node. Proof-checked TLT inclusion still runs on this device.",
  );
  return parts.join(" ");
}

export async function loadRpcEndpoint(storage: VaultStorage): Promise<string | null> {
  const raw = await storage.load();
  if (!raw) return null;
  return validateRpcUrl(raw);
}

export async function saveRpcEndpoint(
  storage: VaultStorage,
  raw: string,
): Promise<string> {
  const url = validateRpcUrl(raw);
  await storage.save(url);
  return url;
}

export async function loadRpcToken(storage: VaultStorage): Promise<string | null> {
  const raw = await storage.load();
  if (!raw) return null;
  const token = raw.trim();
  if (!token) return null;
  if (token.length > MAX_TOKEN_CHARS || /\s/.test(token)) {
    throw new Error("stored RPC token is malformed");
  }
  return token;
}

export async function saveRpcToken(storage: VaultStorage, raw: string): Promise<void> {
  const token = raw.trim();
  if (!token) {
    await storage.clear();
    return;
  }
  if (token.length > MAX_TOKEN_CHARS || /\s/.test(token)) {
    throw new Error("RPC token must be a single token");
  }
  await storage.save(token);
}
