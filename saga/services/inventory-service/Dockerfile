# Multi-stage build. Build context is the WORKSPACE ROOT (saga/) — see
# docker-compose.yml (`context: .`).

FROM rust:1-slim-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p inventory-service

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/inventory-service /usr/local/bin/inventory-service
ENV PORT=3004
ENV NATS_URL=nats://nats:4222
EXPOSE 3004
CMD ["inventory-service"]
