import type { GuildKind } from "../community/types";

export function joinGuild(kind: GuildKind): { status: "PLANNED"; member: false; kind: GuildKind } {
  return { status: "PLANNED", member: false, kind };
}
