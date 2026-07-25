# Multi-stage build. Build context is the WORKSPACE ROOT (event-driven/) — see
# docker-compose.yml (`context: .`).

FROM rust:1-slim-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p orders-service

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/orders-service /usr/local/bin/orders-service
ENV PORT=3003
ENV NATS_URL=nats://nats:4222
EXPOSE 3003
CMD ["orders-service"]
