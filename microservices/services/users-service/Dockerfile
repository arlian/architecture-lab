# Multi-stage build. The build context is the WORKSPACE ROOT (microservices/),
# not this folder, because Cargo needs the workspace manifest and every member to
# resolve the build. See docker-compose.yml, which sets `context: .` accordingly.

FROM rust:1-slim-bookworm AS builder
# reqwest's default TLS backend is OpenSSL on Linux, so the build needs its
# headers. (Locally on Windows it uses SChannel and needs none of this.)
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN cargo build --release -p users-service

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/users-service /usr/local/bin/users-service
ENV PORT=3001
EXPOSE 3001
CMD ["users-service"]
