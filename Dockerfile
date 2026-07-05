# ── Stage 1: Build ──────────────────────────────────
FROM rust:1.88-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY core/Cargo.toml core/
COPY traits/Cargo.toml traits/
COPY runtime/Cargo.toml runtime/
COPY eventbus/Cargo.toml eventbus/
COPY security/Cargo.toml security/
COPY config/Cargo.toml config/
COPY storage/Cargo.toml storage/
COPY database/Cargo.toml database/
COPY observability/Cargo.toml observability/
COPY backends/Cargo.toml backends/
COPY app/Cargo.toml app/

RUN mkdir -p core/src traits/src runtime/src eventbus/src \
    security/src config/src storage/src database/src \
    observability/src backends/src app/src && \
    for d in core traits runtime eventbus security config storage database observability backends app; do \
        echo "// placeholder" > "$d/src/lib.rs" 2>/dev/null || true; \
    done && \
    echo "fn main() {}" > app/src/main.rs && \
    cargo build --release -p lingshu 2>/dev/null || true

COPY . .
RUN cargo build --release -p lingshu

# ── Stage 2: Runtime ────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/lingshu /usr/local/bin/lingshu
COPY config/ /etc/lingshu/config/

RUN mkdir -p /var/lib/lingshu /var/log/lingshu

EXPOSE 8080

ENV LS_ENV=prod

ENTRYPOINT ["/usr/local/bin/lingshu"]
CMD ["--serve", "--addr", "0.0.0.0:8080"]
