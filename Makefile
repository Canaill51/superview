.PHONY: help check build test coverage-audit fmt clippy clean

# Default target
help:
	@echo "superview - Build & Development Commands"
	@echo ""
	@echo "  check          Run every verification gate (scripts/check.sh)"
	@echo "  build          Build the release binary (target/release/superview)"
	@echo "  test           Run the whole test suite (needs ffmpeg on PATH)"
	@echo "  coverage-audit Coverage floor (50 %), cargo audit and cargo deny, as the CI job does"
	@echo "  fmt            Check formatting (cargo fmt --check)"
	@echo "  clippy         Run clippy, warnings are errors"
	@echo "  clean          Remove the build output (cargo clean)"

check:
	scripts/check.sh

build:
	cargo build --release --locked -p superview-app

# SUPERVIEW_REQUIRE_FFMPEG=1: without it every test that needs ffmpeg skips silently.
test:
	SUPERVIEW_REQUIRE_FFMPEG=1 cargo test --workspace --locked

# Needs cargo-llvm-cov, cargo-audit and cargo-deny (cargo install --locked …); versions: rust.yml.
coverage-audit:
	SUPERVIEW_REQUIRE_FFMPEG=1 cargo llvm-cov --workspace --locked --fail-under-lines 50
	cargo audit
	cargo deny --locked check advisories licenses

fmt:
	cargo fmt --all --check

clippy:
	cargo clippy --workspace --all-targets --locked -- -D warnings

clean:
	cargo clean
