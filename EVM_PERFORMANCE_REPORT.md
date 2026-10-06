# EVM performance report

**Maturity:** Experimental measurement, not a capacity claim.
**Profile:** `cargo test -p agora-ovl-evm --lib measured_transfer_batch`
**Build:** debug, unoptimized, `revm` 42.0.1 `SpecId::SHANGHAI`.

| Measurement | Value |
| --- | --- |
| Transactions | 30 sequential EIP-1559 native transfers |
| Gas used | 630000 (`21000 * 30`) |
| Elapsed | 80220 microseconds |
| Observed rate in this run | 30 / 0.080220 seconds |

The elapsed figure is the `ovl-evm-perf` line from
`/opt/cursor/artifacts/ovl-evm-crate-tests.log`. No other throughput, latency,
or mainnet figure was measured. This debug run is not a validator benchmark
and it is not evidence of public-network performance.
