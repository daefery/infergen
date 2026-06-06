# List available recipes.
default:
    @just --list

# --- Setup ---
install:
    pnpm install

# --- Rust ---
build-rust:
    cargo build --workspace

test-rust:
    cargo test --workspace

lint-rust:
    cargo clippy --workspace --all-targets -- -D warnings

fmt-check-rust:
    cargo fmt --all -- --check

fmt:
    cargo fmt --all

deny:
    cargo deny check

# --- JS ---
build-js:
    pnpm -r build

test-js:
    pnpm -r test

typecheck-js:
    pnpm -r typecheck

# --- Aggregate ---
build: build-rust build-js

test: test-rust test-js

lint: lint-rust typecheck-js

# Full local CI parity. Run before pushing.
ci: fmt-check-rust lint deny test build
