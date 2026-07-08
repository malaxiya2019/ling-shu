# ── Stage 1: Build ──
FROM rust:1.88-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev protobuf-compiler && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./

# Copy all crate Cargo.toml files for dependency caching
COPY audit/Cargo.toml backends/Cargo.toml billing/Cargo.toml code-analyzer/Cargo.toml config/Cargo.toml core/Cargo.toml ./
COPY credentials/Cargo.toml database/Cargo.toml distributed/Cargo.toml eventbus/Cargo.toml knowledge-graph/Cargo.toml mcp/Cargo.toml ./
COPY memory/Cargo.toml multimodal/Cargo.toml observability/Cargo.toml orchestrator/Cargo.toml plugin/Cargo.toml polyglot/Cargo.toml ./
COPY prompt/Cargo.toml ratelimit/Cargo.toml runtime/Cargo.toml security/Cargo.toml storage/Cargo.toml tests/Cargo.toml traits/Cargo.toml websocket/Cargo.toml ./
COPY app/Cargo.toml ./

RUN mkdir -p \
    audit/src backends/src billing/src code-analyzer/src config/src core/src \
    credentials/src database/src distributed/src eventbus/src knowledge-graph/src mcp/src \
    memory/src multimodal/src observability/src orchestrator/src plugin/src polyglot/src \
    prompt/src ratelimit/src runtime/src security/src storage/src tests/src \
    traits/src websocket/src app/src tests/src && \
    for d in audit backends billing code-analyzer config core credentials database distributed eventbus knowledge-graph mcp memory multimodal observability orchestrator plugin polyglot prompt ratelimit runtime security storage tests traits websocket app tests; do \
        echo "// placeholder" > "$d/src/lib.rs" 2>/dev/null || true; \
    done && \
    echo "fn main() {}" > app/src/main.rs && \
    cargo build --release -p lingshu 2>/dev/null || true

COPY . .

# ── WebUI WASM Build (trunk) ──
RUN rustup target add wasm32-unknown-unknown &&     cargo install trunk --locked &&     cd webui && trunk build --release

# ── Final Server Build ──
RUN cargo build --release -p lingshu

# ── Stage 2: Runtime ──
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/lingshu /usr/local/bin/lingshu
COPY --from=builder /app/webui/dist/ /webui/dist/
COPY config/ /etc/lingshu/config/

RUN mkdir -p /var/lib/lingshu /var/log/lingshu

EXPOSE 8080 9090

ENV LS_ENV=prod
ENV LINGSHU_CREDENTIAL_MASTER_KEY=change-me-in-production

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -sf http://localhost:8080/health || exit 1

ENTRYPOINT ["/usr/local/bin/lingshu"]
CMD ["--addr", "0.0.0.0:8080"]
