# Multi-stage build. Build context is the WORKSPACE ROOT (cqrs-es/) — see
# docker-compose.yml (`context: .`).

FROM rust:1-slim-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p orders-query-service

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/orders-query-service /usr/local/bin/orders-query-service
ENV PORT=3004
ENV NATS_URL=nats://nats:4222
EXPOSE 3004
CMD ["orders-query-service"]
