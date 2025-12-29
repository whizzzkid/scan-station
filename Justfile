# Task runner recipes for this project.

# Default: show available recipes
_default:
	@just --list

default: _default

# Build the project
build:
	cargo build

# Build optimized binary
build-release:
	cargo build --release

# Run tests
test:
	cargo test

# Format the code (requires rustfmt)
fmt:
	cargo fmt

# Lint with clippy and deny warnings
lint:
  cargo clippy -- -D clippy::pedantic

lint-fix:
	cargo clippy --fix --allow-dirty --allow-staged -- -D clippy::pedantic

# Type-check without building
check:
	cargo check

# Run the binary with optional arguments
# Usage: just run -- https://httpbin.org/get
run *ARGS:
	cargo run -- {{ARGS}}

# Watch for changes and run the app (requires cargo-watch)
watch-run *ARGS:
	cargo watch -x "run -- {{ARGS}}"

# Watch for changes and run tests (requires cargo-watch)
watch-test:
	cargo watch -x "test"

# Install helpful local tools (macOS)
install-tools:
	if ! command -v just >/dev/null; then echo "Installing just via Homebrew"; brew install just; fi
	if ! command -v cargo-watch >/dev/null; then echo "Installing cargo-watch"; cargo install cargo-watch; fi
	rustup component add clippy rustfmt

# Show help
help:
	@echo "Available recipes:"
	@just --list
