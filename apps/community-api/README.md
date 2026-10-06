# Agora community API

**Maturity: Experimental.**

Local HTTP surface for the same community types the PC and phone light
clients import from `apps/shared/community`. It is not a consensus RPC and it
does not store seeds, private keys, or KYC documents.

```bash
cd apps/shared && npm install
node --experimental-strip-types ../community-api/server.ts
```

`AGORA_COMMUNITY_PORT` defaults to `8787`.

| Method | Path | Auth | Body |
| --- | --- | --- | --- |
| POST | `/session/challenge` | no | `{ "address": "agora1…" }` |
| POST | `/session` | wallet signature | challenge, compressed public key, compact signature |
| GET | `/passport` | no | public passports |
| GET | `/passport/private` | bearer session | email and language for the session address |
| GET | `/reputation` | no | category scores |
| GET | `/missions` | no | mission records |
| GET | `/grants` | no | grant records, no disbursement |
| GET | `/bounties` | no | bounties |
| GET | `/academy` | no | courses |
| GET | `/events` | no | events |
| GET | `/merchants` | no | receiving profiles, no merchant keys |
| GET | `/guilds` | no | guild charters |
| GET | `/proposals` | no | advisory and indexed proposals |
| GET | `/treasury` | no | fixture rows until a node treasury read replaces them |
| GET | `/contributions` | no | verifiable contribution history |
| GET | `/hubs` | no | hubs |
| GET | `/forum` | no | posts |
| GET | `/developers` | no | builder directory |
| GET | `/ecosystem` | no | counts |

Rate limit: 30 requests per minute per remote address. Opening a session
requires a valid `agora-community-session-v1` signature. The issued token
expires in 15 minutes and cannot carry a mnemonic.
