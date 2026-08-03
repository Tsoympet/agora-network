# Agora Network node image (RandomX + RocksDB).
# Build: docker build -t agora-node .
# Run:   docker run --rm -p 8545:8545 -p 16111:16111 agora-node

FROM rust:1.85-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake clang pkg-config libclang-dev build-essential \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .cargo ./.cargo
COPY core ./core
COPY infrastructure ./infrastructure
ENV LIBCLANG_PATH=/usr/lib/llvm-18/lib
RUN cargo build --release -p agora-node -p agora-dns-seeder -p agora-miner-sidecar -p agora-testnet-faucet

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libstdc++6 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /agora
COPY --from=builder /src/target/release/agora-node /usr/local/bin/
COPY --from=builder /src/target/release/agora-dns-seeder /usr/local/bin/
COPY --from=builder /src/target/release/agora-miner /usr/local/bin/
COPY --from=builder /src/target/release/agora-testnet-faucet /usr/local/bin/
COPY docs/genesis/testnet.genesis.json /agora/genesis/testnet.genesis.json
ENV AGORA_NETWORK=testnet \
    AGORA_DATA=/data \
    AGORA_RPC_BIND=0.0.0.0:8545 \
    AGORA_RPC_ALLOW_PUBLIC_BIND=1 \
    AGORA_LISTEN=/ip4/0.0.0.0/tcp/16111 \
    AGORA_GENESIS_FILE=/agora/genesis/testnet.genesis.json
VOLUME ["/data"]
EXPOSE 8545 16111
ENTRYPOINT ["agora-node"]
