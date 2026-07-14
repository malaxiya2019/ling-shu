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
RUN rustup target add wasm32-unknown-unknown \
    && curl -sL https://github.com/trunk-rs/trunk/releases/download/v0.21.8/trunk-x86_64-unknown-linux-gnu.tar.gz \
       | tar xz -C /usr/local/bin \
    && cd webui && trunk build --release

# ── Final Server Build ──
ARG FEATURES=""
RUN cargo build --release -p lingshu ${FEATURES:+--features "$FEATURES"}

# ── Stage 2: Runtime ──
FROM debian:bookworm-slim

# 安装运行时依赖: Node.js (agent-device) + Python (SimpleCAD)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl tzdata \
    python3 python3-pip python3-venv \
    && rm -rf /var/lib/apt/lists/*

# 安装 Node.js 22.x + agent-device
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && npm install -g agent-device \
    && rm -rf /var/lib/apt/lists/*

# 安装 SimpleCADAPI
RUN pip3 install --break-system-packages simplecadapi mcp

COPY --from=builder /app/target/release/lingshu /usr/local/bin/lingshu
COPY --from=builder /app/webui/dist/ /webui/dist/
COPY config/ /etc/lingshu/config/

# 复制 SimpleCAD MCP 服务器
COPY plugins/simplecad-plugin/mcp-server/ /opt/lingshu/plugins/simplecad-mcp/
RUN pip3 install --break-system-packages -e /opt/lingshu/plugins/simplecad-mcp/

RUN mkdir -p /var/lib/lingshu /var/log/lingshu

EXPOSE 8080 9090

ENV LS_ENV=prod
ENV LS_LOG_LEVEL=info
ENV LINGSHU_CREDENTIAL_MASTER_KEY=change-me-in-production
ENV PATH="/usr/local/bin:${PATH}"

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -sf http://localhost:8080/health || exit 1

ENTRYPOINT ["/usr/local/bin/lingshu"]
CMD ["--addr", "0.0.0.0:8080"]
