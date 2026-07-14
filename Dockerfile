# syntax=docker/dockerfile:1.7
# Multi-stage Dockerfile for the substrate gateway + CLI.
# Resulting image is ~120 MB (Debian-slim distroless-style runtime).

# -------- Stage 1: build --------
FROM rust:1.82-slim-bookworm AS builder

# Build deps
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache deps separately from source
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY fuzz fuzz
RUN mkdir -p fuzz && touch fuzz/.gitkeep && \
    cargo fetch --locked

# Build release binaries (use workspace settings for max perf)
COPY . .
RUN cargo build --release \
        -p driver-cli \
        -p psub-gateway \
        -p driver-http

# Strip debug symbols to shrink image
RUN strip target/release/substrate && \
    strip target/release/substrate-gateway && \
    strip target/release/substrate-http

# -------- Stage 2: runtime --------
FROM debian:bookworm-slim AS runtime

# Add a non-root user
RUN groupadd --system --gid 1001 substrate && \
    useradd  --system --uid 1001 --gid substrate --create-home substrate

# CA certs for upstream provider TLS, /tini for signal handling
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl tini \
    && rm -rf /var/lib/apt/lists/*

# Substrate config + data
ENV SUBSTRATE_HOME=/var/lib/substrate \
    RUST_LOG=info \
    RUST_BACKTRACE=1 \
    PSUB_GATEWAY_BIND=0.0.0.0:8080

WORKDIR /var/lib/substrate

# Copy binaries from builder
COPY --from=builder --chown=substrate:substrate \
    /build/target/release/substrate            /usr/local/bin/substrate
COPY --from=builder --chown=substrate:substrate \
    /build/target/release/substrate-gateway    /usr/local/bin/substrate-gateway
COPY --from=builder --chown=substrate:substrate \
    /build/target/release/substrate-http     /usr/local/bin/substrate-http

COPY --chown=substrate:substrate docs/openapi.yaml  /etc/substrate/openapi.yaml

USER substrate

EXPOSE 8080

# Healthcheck hits the in-process liveness probe (cheap, no auth).
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD ["curl", "--fail", "--silent", "http://127.0.0.1:8080/healthz"]

ENTRYPOINT ["/usr/bin/tini", "--"]

# Default process is the HTTP gateway.
CMD ["substrate-gateway"]
