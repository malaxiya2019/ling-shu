# ── Stage 1: Build ──
FROM rust:1.88-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./

# Copy all crate Cargo.toml files for dependency caching
COPY core/Cargo.toml traits/Cargo.toml runtime/Cargo.toml eventbus/Cargo.toml ./
COPY security/Cargo.toml config/Cargo.toml storage/Cargo.toml database/Cargo.toml ./
COPY observability/Cargo.toml backends/Cargo.toml plugin/Cargo.toml ./
COPY orchestrator/Cargo.toml polyglot/Cargo.toml distributed/Cargo.toml ./
COPY app/Cargo.toml tests/Cargo.toml ./

RUN mkdir -p core/src traits/src runtime/src eventbus/src \
    security/src config/src storage/src database/src \
    observability/src backends/src plugin/src \
    orchestrator/src polyglot/src distributed/src \
    app/src tests/src && \
    for d in core traits runtime eventbus security config storage database \
             observability backends plugin orchestrator polyglot distributed app tests; do \
        echo "// placeholder" > "$d/src/lib.rs" 2>/dev/null || true; \
    done && \
    echo "fn main() {}" > app/src/main.rs && \
    cargo build --release -p lingshu 2>/dev/null || true

COPY . .
RUN cargo build --release -p lingshu

# ── Stage 2: Runtime ──
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/lingshu /usr/local/bin/lingshu
COPY config/ /etc/lingshu/config/

RUN mkdir -p /var/lib/lingshu /var/log/lingshu

EXPOSE 8080 9090

ENV LS_ENV=prod

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -sf http://localhost:8080/health || exit 1

ENTRYPOINT ["/usr/local/bin/lingshu"]
CMD ["serve", "--addr", "0.0.0.0:8080"]
