# Multi-stage build. Build context is the WORKSPACE ROOT (concurrency/) — see
# docker-compose.yml (`context: .`).

FROM rust:1-slim-bookworm AS builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN cargo build --release -p pessimistic-service

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/pessimistic-service /usr/local/bin/pessimistic-service
ENV PORT=3022
EXPOSE 3022
CMD ["pessimistic-service"]
