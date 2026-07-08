# Testing

## Running Tests

```bash
# All tests
cargo test --all --all-features

# Quick tests (no feature gates)
cargo test --all

# Specific crate
cargo test -p lingshu-runtime

# API tests
cargo test -p lingshu

# With coverage
make coverage
```

## Test Structure

- **Unit tests**: Inside each crate's `tests` module
- **Integration tests**: `tests/` crate
- **E2E tests**: `tests/src/*_e2e.rs`
- **Fuzz tests**: `fuzz/` directory

## CI Pipeline

See `.github/workflows/ci.yml`:

1. Check formatting
2. Clippy linting
3. Build all features
4. Run unit + integration tests
5. Run fuzz targets
6. Build docs
