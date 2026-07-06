.PHONY: all build check test clean fmt lint doc ci-setup run repl serve help

SHELL := /bin/bash

all: check test build

# ── Build ──

build:
	cargo build --all-features

build-release:
	cargo build --release --all-features

# ── Check ──

check:
	cargo check --all-features
	cargo check --all-targets --all-features

ci-setup:
	rustup component add rustfmt clippy
	cargo install cargo-audit cargo-deny 2>/dev/null || true

# ── Test ──

test:
	cargo test --all --all-features

test-quick:
	cargo test --all

test-runtime:
	cargo test -p lingshu-runtime

test-api:
	cargo test -p lingshu

# ── Lint ──

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all --check

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

lint: fmt-check clippy

# ── Doc ──

doc:
	cargo doc --no-deps --all-features

doc-open: doc
	xdg-open target/doc/lingshu/index.html 2>/dev/null || open target/doc/lingshu/index.html

# ── Run ──

run: build
	cargo run -p lingshu

repl: build
	cargo run -p lingshu -- --repl

serve: build
	cargo run -p lingshu -- --serve --addr 0.0.0.0:8080

prod: build-release
	cargo run -p lingshu --release -- -e prod --addr 0.0.0.0:8080

# ── Clean ──

clean:
	cargo clean

# ── Utils ──

outdated:
	cargo outdated 2>/dev/null || cargo install cargo-outdated && cargo outdated

tree:
	cargo tree

# ── Help ──

help:
	@echo "Lingshu Dev Commands"
	@echo "━━━━━━━━━━━━━━━━━━━"
	@echo "  make build          — Build all crates"
	@echo "  make build-release  — Release build"
	@echo "  make check          — Check compilation"
	@echo "  make test           — Run all tests"
	@echo "  make test-quick     — Quick tests (no features)"
	@echo "  make test-api       — API tests only"
	@echo "  make test-runtime   — Runtime tests only"
	@echo "  make fmt            — Format code"
	@echo "  make fmt-check      — Check formatting"
	@echo "  make clippy         — Run Clippy"
	@echo "  make lint           — fmt-check + clippy"
	@echo "  make doc            — Build docs"
	@echo "  make run            — Run HTTP server"
	@echo "  make repl           — Run REPL mode"
	@echo "  make clean          — Clean build artifacts"
	@echo ""
	@echo "Docker:"
	@echo "━━━━━━━━━"
	@echo "  make docker-build  — Build Docker image"
	@echo "  make docker-up     — Start containers"
	@echo "  make docker-down   — Stop containers"
	@echo "  make docker-logs   — Tail logs"


# ── Docker ──

docker-build:
	docker build -t lingshu:latest .

docker-up:
	docker compose up -d

docker-down:
	docker compose down

docker-logs:
	docker compose logs -f

docker-restart: docker-down docker-up

