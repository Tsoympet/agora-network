# Data plane

**Maturity: Scaffold.**

`DataPlane` is `device`, `infrastructure`, or `on-chain`. Feature descriptors
live in `features.ts`. Light-client capability rows use the same field.

`createDeviceInfrastructureClient` speaks HTTP to an infrastructure origin. It
does not import the indexer or the forum server. See
[`docs/core/AGORA_DATA_PLANE_SPLIT.md`](../../../docs/core/AGORA_DATA_PLANE_SPLIT.md).
