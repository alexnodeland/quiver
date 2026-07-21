# Quiver Makefile
# Common development commands

.PHONY: all build test check fmt lint lint-fix clippy doc bench bench-simd bench-rt coverage clean setup help
.PHONY: install-hooks changelog examples wasm wasm-dev wasm-check ts-check check-nostd
.PHONY: test-browser browser-synth

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

# Run all checks (format, lint, test).
#
# Deliberately does NOT depend on `check-nostd`: that target needs the
# thumbv7em-none-eabihf target installed (`rustup target add
# thumbv7em-none-eabihf`) and, if Homebrew Rust is shadowing rustup on your
# PATH, the rustup toolchain's own cargo/rustc binaries -- not guaranteed to
# be set up on every machine, and not worth slowing down the everyday
# fmt/lint/test loop for. Run `make check-nostd` separately (CI runs it as
# its own `no_std` job) to validate the no_std/alloc feature tiers.
check: fmt-check lint test
	@echo "All checks passed!"

# no_std / alloc-tier check (Q137): mirrors CI's `no_std` job so you can
# reproduce it locally. Requires the thumbv7em-none-eabihf target:
#   rustup target add thumbv7em-none-eabihf
# On a machine where Homebrew Rust shadows rustup on PATH (Homebrew's
# cargo/rustc have no cross targets), invoke the rustup toolchain directly,
# e.g.: PATH="$$(dirname $$(rustup which cargo)):$$PATH" make check-nostd
#
# `--crate-type rlib` overrides the crate's declared `cdylib` (see the [lib]
# comment in Cargo.toml / Q140): checking the unmodified cdylib+rlib
# crate-type on a no_std build fails with spurious "no global memory
# allocator" / "panic_handler function required" errors unrelated to this
# crate's actual no_std code, since rustc demands an allocator/panic-handler
# for any cdylib artifact.
#
# Fallback: if thumbv7em-none-eabihf isn't installed and you just want a
# rough host-target sanity check instead (weaker signal -- the host target
# has 64-bit atomics and an allocator a real embedded target may lack), drop
# `--target thumbv7em-none-eabihf` from both lines below.
check-nostd:
	cargo rustc --lib --crate-type rlib --no-default-features --target thumbv7em-none-eabihf -- --emit=metadata
	cargo rustc --lib --crate-type rlib --no-default-features --features alloc --target thumbv7em-none-eabihf -- --emit=metadata

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

# Run benchmarks (scalar build, then SIMD build) so both code paths are
# exercised. For a direct scalar-vs-SIMD comparison of the block ops, use
# `make bench-simd` (it saves and compares criterion baselines).
bench:
	cargo bench
	cargo bench --features simd

# Scalar-vs-SIMD A/B for the block operations (Q116). Saves the scalar build as
# the `scalar` baseline, then re-runs the SAME bench names against the SIMD
# build so criterion prints the per-op delta. Scoped to the `simd` bench group
# so it stays fast.
bench-simd:
	cargo bench -- simd --save-baseline scalar
	cargo bench --features simd -- simd --baseline scalar

# Real-time compliance gate: measure the worst-case chain and populated
# polyphony against the real-time budget. MUST run in release — debug builds
# skip the wall-clock assertion (see tests/realtime_compliance.rs).
bench-rt:
	cargo test --release --test realtime_compliance -- --nocapture

# Run benchmark tests only (no actual benchmarking)
bench-test:
	cargo bench -- --test

# Run tests with coverage (requires: rustup component add llvm-tools-preview && cargo install cargo-llvm-cov)
coverage:
	cargo llvm-cov --all-features --ignore-filename-regex 'src/wasm/.*' --fail-under-lines 80

# Run coverage and generate HTML report
coverage-html:
	cargo llvm-cov --all-features --ignore-filename-regex 'src/wasm/.*' --html
	@echo "Coverage report: target/llvm-cov/html/index.html"

# Clean build artifacts
clean:
	cargo clean
	rm -rf docs/book/
	rm -rf pkg/
	rm -f tarpaulin-report.html
	rm -f packages/@quiver-dsp/wasm/quiver*.js packages/@quiver-dsp/wasm/quiver*.wasm packages/@quiver-dsp/wasm/quiver*.d.ts

# Setup development environment
setup: install-hooks
	rustup component add rustfmt clippy llvm-tools-preview
	@echo "Installing cargo-llvm-cov for coverage..."
	cargo install cargo-llvm-cov || true
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

# Build WASM package (release).
#
# The `release` profile is now speed-optimized (opt-level = 3) and `.cargo/
# config.toml` enables `+simd128`, so this ships fast, SIMD-capable wasm — the
# right default for real-time DSP. This produces a LARGER `.wasm` than the old
# size-optimized default. If you need the smallest binary instead, build the
# library with the size profile: `cargo build --profile wasm-size
# --no-default-features --features wasm --target wasm32-unknown-unknown`
# (wasm-pack always uses the `release` profile).
wasm:
	wasm-pack build --target web --no-default-features --features wasm
	cp pkg/quiver.js pkg/quiver.d.ts pkg/quiver_bg.wasm pkg/quiver_bg.wasm.d.ts packages/@quiver-dsp/wasm/
	@echo "WASM package built: packages/@quiver-dsp/wasm/"

# Build WASM package (development, faster)
#
# `--dev` must come before the cargo passthrough flags (`--no-default-features
# --features wasm`), not after: wasm-pack's CLI (>=0.13) treats the first
# unrecognized flag as the start of its trailing `[EXTRA_OPTIONS]...` capture
# (everything from there on is forwarded verbatim to `cargo build`), so
# `--no-default-features` swallows `--dev` into that passthrough instead of
# wasm-pack's own `--dev` option -- silently producing a release build (or a
# `cargo build` argument error) instead of the intended dev build.
wasm-dev:
	wasm-pack build --dev --target web --no-default-features --features wasm
	cp pkg/quiver.js pkg/quiver.d.ts pkg/quiver_bg.wasm pkg/quiver_bg.wasm.d.ts packages/@quiver-dsp/wasm/
	@echo "WASM package built (dev): packages/@quiver-dsp/wasm/"

# Build the @quiver-dsp/wasm TS wrapper (dist/worklet.js etc.) on top of the
# wasm-pack output. The worklet integration test loads dist/worklet.js, so the
# browser test targets depend on this, not just `wasm`.
wasm-ts: wasm
	npm install --silent
	npm run build:wasm:ts

# Check WASM compilation without building
wasm-check:
	cargo check --target wasm32-unknown-unknown --no-default-features --features wasm

# Check TypeScript compilation
ts-check:
	@echo "Checking TypeScript..."
	@cd packages/@quiver-dsp/types && npx tsc --noEmit 2>/dev/null || (npm install --silent && npx tsc --noEmit)
	@echo "TypeScript OK"

# Run browser tests with Playwright
test-browser: wasm-ts
	@echo "Running browser tests..."
	@cd demos/browser/tests && npm install --silent && npx playwright install --with-deps chromium && npx playwright test --project=chromium

# Run browser tests on all browsers
test-browser-all: wasm-ts
	@echo "Running browser tests on all browsers..."
	@cd demos/browser/tests && npm install --silent && npx playwright install --with-deps && npx playwright test

# Run browser synth demo
browser-synth: wasm
	@echo "Starting browser synth demo..."
	@cd demos/browser && npm install --silent && npm run dev

# Run demo
demo: browser-synth

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
	@echo "  make ts-check     - Check TypeScript compilation"
	@echo "  make clean        - Clean build artifacts"
	@echo ""
	@echo "Testing:"
	@echo "  make test         - Run all tests"
	@echo "  make test-verbose - Run tests with output"
	@echo "  make test-doc     - Run documentation tests"
	@echo "  make coverage     - Run tests with coverage (80% line threshold)"
	@echo "  make coverage-html- Generate HTML coverage report"
	@echo "  make bench        - Run benchmarks (scalar + SIMD builds)"
	@echo "  make bench-simd   - Scalar-vs-SIMD A/B (criterion baselines)"
	@echo "  make bench-rt     - Real-time compliance gate (release)"
	@echo ""
	@echo "Code Quality:"
	@echo "  make check        - Run all checks (fmt, lint, test)"
	@echo "  make check-nostd  - Verify no_std/alloc feature tiers (thumbv7em-none-eabihf)"
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
	@echo "Demos:"
	@echo "  make browser-synth- Run browser synth demo (demos/browser/)"
	@echo ""
	@echo "Browser Testing:"
	@echo "  make test-browser - Run browser tests (Chromium only)"
	@echo "  make test-browser-all - Run browser tests (all browsers)"
	@echo ""
	@echo "Setup:"
	@echo "  make setup        - Setup development environment"
	@echo "  make install-hooks- Install git hooks"
	@echo "  make changelog    - Generate changelog (requires git-cliff)"
	@echo ""
	@echo "Watching:"
	@echo "  make watch        - Watch and run tests on changes"
	@echo "  make watch-check  - Watch and check on changes"
