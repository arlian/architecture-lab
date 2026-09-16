# Multi-stage build. Build context is the WORKSPACE ROOT (concurrency/) — see
# docker-compose.yml (`context: .`). This one is a CLI, not a server: it runs,
# prints a verdict, and exits non-zero if anything was oversold.

FROM rust:1-slim-bookworm AS builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN cargo build --release -p race-runner

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/race-runner /usr/local/bin/race-runner
ENTRYPOINT ["race-runner"]
