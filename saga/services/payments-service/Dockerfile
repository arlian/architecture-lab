# Multi-stage build. Build context is the WORKSPACE ROOT (saga/) — see
# docker-compose.yml (`context: .`).

FROM rust:1-slim-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p payments-service

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/payments-service /usr/local/bin/payments-service
ENV PORT=3005
ENV NATS_URL=nats://nats:4222
EXPOSE 3005
CMD ["payments-service"]
