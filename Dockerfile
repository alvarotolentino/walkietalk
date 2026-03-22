# Multi-stage build for WalkieTalk Rust services
FROM rust:1.88-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p walkietalk-auth -p walkietalk-signaling -p walkietalk-zmq-proxy

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/walkietalk-auth /app/walkietalk-auth
COPY --from=builder /app/target/release/walkietalk-signaling /app/walkietalk-signaling
COPY --from=builder /app/target/release/walkietalk-zmq-proxy /app/walkietalk-zmq-proxy
WORKDIR /app
