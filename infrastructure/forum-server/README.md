# Forum server

**Maturity: Scaffold.**

Holds community posts for an operator. Posts are not chain transactions, votes,
or treasury spends. The PC and phone render posts through the HTTP facade in
`apps/shared/data-plane` and do not import this package.

Trust: the operator can omit, reorder, or alter a post. Administrative civic
forum RPC on a full node is separate local node state. It is not this service
and it is not embedded in the light clients.

See [`docs/core/AGORA_DATA_PLANE_SPLIT.md`](../../docs/core/AGORA_DATA_PLANE_SPLIT.md).
