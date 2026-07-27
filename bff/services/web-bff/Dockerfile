# Multi-stage build. Build context is the WORKSPACE ROOT (bff/) — see
# docker-compose.yml (`context: .`).
#
# web-bff needs OpenSSL at runtime, same as orders-service, because it makes
# outbound HTTPS-capable calls via reqwest to all three backend services.

FROM rust:1-slim-bookworm AS builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN cargo build --release -p web-bff

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/web-bff /usr/local/bin/web-bff
ENV PORT=3004
# ORDERS_URL / USERS_URL / CATALOG_URL are injected by docker-compose.
EXPOSE 3004
CMD ["web-bff"]
