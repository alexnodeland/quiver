# Quiver Makefile
# Common development commands

.PHONY: all build test check fmt lint lint-fix clippy doc bench coverage clean setup help
.PHONY: install-hooks changelog examples wasm wasm-dev wasm-check

# Default target
all: check

# Build the project
build:
	cargo build --all-features

# Build in release mode
release:
	cargo build --release --all-features

# Run all tests
test:
	cargo test --all-features

# Run tests with verbose output
test-verbose:
	cargo test --all-features -- --nocapture

# Run doc tests only
test-doc:
	cargo test --doc --all-features

# Run all checks (format, lint, test)
check: fmt-check lint test
	@echo "All checks passed!"

# Format code
fmt:
	cargo fmt --all

# Check formatting without modifying
fmt-check:
	cargo fmt --all -- --check

# Run clippy linter
lint:
	cargo clippy --all-features -- -D warnings

# Fix clippy lint issues automatically
lint-fix:
	cargo clippy --all-features --fix --allow-dirty --allow-staged

# Alias for lint
clippy: lint

# Build documentation
doc:
	cargo doc --no-deps --all-features --open

# Build documentation without opening
doc-build:
	cargo doc --no-deps --all-features

# Build mdbook documentation
doc-book:
	mdbook build docs/

# Serve mdbook documentation locally
doc-serve:
	mdbook serve docs/

# Run benchmarks
bench:
	cargo bench

# Run benchmark tests only (no actual benchmarking)
bench-test:
	cargo bench -- --test

# Run tests with coverage
coverage:
	cargo tarpaulin --all-features --fail-under 80

# Run coverage and generate HTML report
coverage-html:
	cargo tarpaulin --all-features --out Html

# Clean build artifacts
clean:
	cargo clean
	rm -rf docs/book/
	rm -f tarpaulin-report.html

# Setup development environment
setup: install-hooks
	rustup component add rustfmt clippy
	@echo "Installing cargo-tarpaulin for coverage..."
	cargo install cargo-tarpaulin || true
	@echo "Installing mdbook for documentation..."
	cargo install mdbook mdbook-mermaid || true
	@echo "Installing git-cliff for changelog generation..."
	cargo install git-cliff || true
	@echo "Development environment ready!"

# Install git hooks
install-hooks:
	@echo "Installing git hooks..."
	@mkdir -p .git/hooks
	@cp .githooks/pre-commit .git/hooks/pre-commit 2>/dev/null || \
		echo '#!/bin/sh\nmake pre-commit' > .git/hooks/pre-commit
	@chmod +x .git/hooks/pre-commit
	@echo "Git hooks installed!"

# Pre-commit hook target
pre-commit: fmt-check lint
	@echo "Pre-commit checks passed!"

# Generate changelog from git history using git-cliff
changelog:
	git cliff --output .github/CHANGELOG.md
	@echo "Changelog updated: .github/CHANGELOG.md"

# Build and run all examples
examples:
	cargo build --examples --all-features

# Run a specific example (usage: make run-example NAME=simple_patch)
run-example:
	cargo run --example $(NAME)

# Quick taste example
quick-taste:
	cargo run --example quick_taste

# Watch for changes and run tests
watch:
	cargo watch -x "test --all-features"

# Watch for changes and check
watch-check:
	cargo watch -x "check --all-features"

# Build WASM package (release)
wasm:
	wasm-pack build --target web --no-default-features --features wasm
	cp pkg/quiver.js pkg/quiver.d.ts pkg/quiver_bg.wasm pkg/quiver_bg.wasm.d.ts packages/@quiver/wasm/
	@echo "WASM package built: packages/@quiver/wasm/"

# Build WASM package (development, faster)
wasm-dev:
	wasm-pack build --target web --no-default-features --features wasm --dev
	cp pkg/quiver.js pkg/quiver.d.ts pkg/quiver_bg.wasm pkg/quiver_bg.wasm.d.ts packages/@quiver/wasm/
	@echo "WASM package built (dev): packages/@quiver/wasm/"

# Check WASM compilation without building
wasm-check:
	cargo check --target wasm32-unknown-unknown --no-default-features --features wasm

# Print help
help:
	@echo "Quiver Development Commands"
	@echo ""
	@echo "Building:"
	@echo "  make build        - Build the project"
	@echo "  make release      - Build in release mode"
	@echo "  make wasm         - Build WASM package (release)"
	@echo "  make wasm-dev     - Build WASM package (development)"
	@echo "  make wasm-check   - Check WASM compilation"
	@echo "  make clean        - Clean build artifacts"
	@echo ""
	@echo "Testing:"
	@echo "  make test         - Run all tests"
	@echo "  make test-verbose - Run tests with output"
	@echo "  make test-doc     - Run documentation tests"
	@echo "  make coverage     - Run tests with coverage (80% threshold)"
	@echo "  make coverage-html- Generate HTML coverage report"
	@echo "  make bench        - Run benchmarks"
	@echo ""
	@echo "Code Quality:"
	@echo "  make check        - Run all checks (fmt, lint, test)"
	@echo "  make fmt          - Format code"
	@echo "  make fmt-check    - Check formatting"
	@echo "  make lint         - Run clippy"
	@echo "  make lint-fix     - Fix clippy issues automatically"
	@echo ""
	@echo "Documentation:"
	@echo "  make doc          - Build and open rustdoc"
	@echo "  make doc-build    - Build rustdoc only"
	@echo "  make doc-book     - Build mdbook"
	@echo "  make doc-serve    - Serve mdbook locally"
	@echo ""
	@echo "Examples:"
	@echo "  make examples     - Build all examples"
	@echo "  make quick-taste  - Run quick_taste example"
	@echo "  make run-example NAME=<name> - Run specific example"
	@echo ""
	@echo "Setup:"
	@echo "  make setup        - Setup development environment"
	@echo "  make install-hooks- Install git hooks"
	@echo "  make changelog    - Generate changelog (requires git-cliff)"
	@echo ""
	@echo "Watching:"
	@echo "  make watch        - Watch and run tests on changes"
	@echo "  make watch-check  - Watch and check on changes"
