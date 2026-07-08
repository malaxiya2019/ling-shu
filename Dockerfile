# ── Stage 1: Chef (dependency resolver) ──
FROM rust:1.88-slim-bookworm AS chef

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev protobuf-compiler curl \
    && rm -rf /var/lib/apt/lists/* \
    && cargo install cargo-chef --locked

WORKDIR /app

# ── Planner ──
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Builder ──
FROM chef AS builder

COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .

# ── WebUI WASM Build (trunk) ──
# Use pre-built trunk binary for faster build (avoid compiling from source)
RUN rustup target add wasm32-unknown-unknown \
    && curl -sL https://github.com/trunk-rs/trunk/releases/download/v0.21.8/trunk-x86_64-unknown-linux-gnu.tar.gz \
       | tar xz -C /usr/local/bin \
    && cd webui && trunk build --release

# ── Final Server Build ──
ARG FEATURES=""
RUN cargo build --release -p lingshu ${FEATURES:+--features "$FEATURES"}

# ── Stage 2: Runtime ──
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl tzdata && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/lingshu /usr/local/bin/lingshu
COPY --from=builder /app/webui/dist/ /webui/dist/
COPY config/ /etc/lingshu/config/

RUN mkdir -p /var/lib/lingshu /var/log/lingshu

EXPOSE 8080 9090

ENV LS_ENV=prod
ENV LS_LOG_LEVEL=info
ENV LINGSHU_CREDENTIAL_MASTER_KEY=change-me-in-production

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -sf http://localhost:8080/health || exit 1

ENTRYPOINT ["/usr/local/bin/lingshu"]
CMD ["--addr", "0.0.0.0:8080"]
