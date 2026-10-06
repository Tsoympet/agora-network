/**
 * Three places Agora data is allowed to live.
 * A PC or phone may show a surface and still must not run the service that owns it.
 */

export const DATA_PLANES = ["device", "infrastructure", "on-chain"] as const;

export type DataPlane = (typeof DATA_PLANES)[number];

export function planeLabel(plane: DataPlane): string {
  switch (plane) {
    case "device":
      return "device";
    case "infrastructure":
      return "infrastructure";
    case "on-chain":
      return "on-chain";
  }
}

/** Import path fragments that must never appear in a device package. */
export const DEVICE_FORBIDDEN_IMPLEMENTATIONS = [
  "infrastructure/indexer",
  "infrastructure/forum-server",
  "infrastructure/grant-admin",
  "infrastructure/event-service",
  "infrastructure/notification-dispatch",
  "infrastructure/search",
  "infrastructure/moderation",
  "apps/community-api",
  "community-api/server",
] as const;
