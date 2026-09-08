# Multi-stage build. Build context is the WORKSPACE ROOT (fan-out/) — see
# docker-compose.yml (`context: .`).

FROM rust:1-slim-bookworm AS builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN cargo build --release -p catalog-provider

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/catalog-provider /usr/local/bin/catalog-provider
ENV PORT=3011
EXPOSE 3011
CMD ["catalog-provider"]
