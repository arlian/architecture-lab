# Multi-stage build. Build context is the WORKSPACE ROOT (bff/) — see
# docker-compose.yml (`context: .`).
#
# mobile-bff needs OpenSSL at runtime, same as orders-service and web-bff,
# because it makes outbound HTTPS-capable calls via reqwest.

FROM rust:1-slim-bookworm AS builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN cargo build --release -p mobile-bff

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/mobile-bff /usr/local/bin/mobile-bff
ENV PORT=3005
# ORDERS_URL / USERS_URL are injected by docker-compose.
EXPOSE 3005
CMD ["mobile-bff"]
