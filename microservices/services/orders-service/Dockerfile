# Multi-stage build. Build context is the WORKSPACE ROOT (microservices/) — see
# docker-compose.yml (`context: .`).
#
# Orders is the one service that actually needs OpenSSL at runtime, because it
# makes outbound HTTPS-capable calls to the other services via reqwest.

FROM rust:1-slim-bookworm AS builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN cargo build --release -p orders-service

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/orders-service /usr/local/bin/orders-service
ENV PORT=3003
# USERS_URL / CATALOG_URL are injected by docker-compose (service-name DNS).
EXPOSE 3003
CMD ["orders-service"]
