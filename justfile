# justfile — Developer workflow recipes for remem
#
# Install just: cargo install just
# List all recipes: just --list
# Run a recipe: just <recipe>

# Default recipe: run full pre-commit checks
default: check

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------

# Build the entire workspace (debug)
build:
    cargo build --workspace

# Build the entire workspace (release, optimised)
build-release:
    cargo build --workspace --release

# Check compilation without producing binaries (fastest feedback loop)
check:
    cargo check --workspace --all-targets

# Generate documentation for the workspace
doc:
    cargo doc --workspace --no-deps --document-private-items
    @echo "Docs generated at target/doc/"

# Open generated documentation in the browser
doc-open:
    cargo doc --workspace --no-deps --open

# ---------------------------------------------------------------------------
# Test
# ---------------------------------------------------------------------------

# Run all workspace tests
test:
    cargo test --workspace

# Run tests with output shown (for debugging)
test-verbose:
    cargo test --workspace -- --nocapture

# Run tests for a specific crate
test-crate crate:
    cargo test -p {{crate}}

# Run a specific test by name
test-one name:
    cargo test --workspace -- {{name}}

# Run tests with coverage report (requires cargo-llvm-cov)
test-coverage:
    cargo llvm-cov --workspace --html
    @echo "Coverage report at target/llvm-cov/html/index.html"

# ---------------------------------------------------------------------------
# Lint & Format
# ---------------------------------------------------------------------------

# Run all lints (format check + clippy + doc warnings)
lint: fmt-check clippy doc-lint

# Check formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# Auto-format all Rust code
fmt:
    cargo fmt --all

# Run clippy with deny-warnings (matches CI)
clippy:
    cargo clippy --workspace --all-targets -- -Dwarnings

# Check documentation builds cleanly
doc-lint:
    RUSTDOCFLAGS="-Dwarnings" cargo doc --workspace --no-deps

# ---------------------------------------------------------------------------
# Clean
# ---------------------------------------------------------------------------

# Remove all build artifacts (debug + release)
clean:
    cargo clean
    @echo "Build artifacts cleaned."

# Deep clean: build artifacts + generated files + caches
clean-all: clean
    @echo "Removing additional caches..."
    -rm -rf target/
    -rm -rf sdk/python/.pytest_cache sdk/python/__pycache__ sdk/python/*.egg-info
    -rm -rf sdk/typescript/node_modules sdk/typescript/dist
    -rm -rf .ruff_cache
    @echo "Deep clean complete."

# ---------------------------------------------------------------------------
# Security & Compliance
# ---------------------------------------------------------------------------

# Audit dependencies for known vulnerabilities (requires cargo-audit)
audit:
    cargo audit

# Run cargo-deny checks (licenses, advisories, sources)
deny:
    cargo deny check advisories licenses sources

# Check for unused dependencies (requires cargo-udeps + nightly)
udeps:
    cargo +nightly udeps --workspace

# ---------------------------------------------------------------------------
# Pre-commit / Pre-push
# ---------------------------------------------------------------------------

# Full pre-commit check: format, lint, test, deny
pre-commit: fmt clippy test deny
    @echo "✅ All pre-commit checks passed."

# Quick pre-push check: format check, clippy, fast compile
pre-push: fmt-check clippy check
    @echo "✅ Pre-push checks passed."

# ---------------------------------------------------------------------------
# Run
# ---------------------------------------------------------------------------

# Start the REST API server (default port 7474)
serve project="default":
    cargo run -p rememhq-api -- --project {{project}}

# Start the MCP server (stdio transport)
mcp:
    cargo run -p rememhq-mcp

# Run the CLI
cli *args:
    cargo run -p rememhq-cli -- {{args}}

# Run the CLI doctor check
doctor:
    cargo run -p rememhq-cli -- doctor --ping

# ---------------------------------------------------------------------------
# Release
# ---------------------------------------------------------------------------

# Bump workspace version (requires cargo-edit)
bump-version version:
    @echo "Bumping workspace version to {{version}}..."
    sed -i 's/^version = ".*"/version = "{{version}}"/' Cargo.toml
    cargo check --workspace
    @echo "Version bumped to {{version}}. Don't forget to update CHANGELOG.md."

# Build release binaries for all targets
release-build:
    cargo build --workspace --release
    @echo "Release binaries at target/release/"

# ---------------------------------------------------------------------------
# SDK
# ---------------------------------------------------------------------------

# Build and test the Python SDK
sdk-python:
    cd sdk/python && pip install -e ".[dev]" && pytest tests/ -v

# Build and test the TypeScript SDK
sdk-typescript:
    cd sdk/typescript && npm install && npm run build && npm test

# ---------------------------------------------------------------------------
# Utilities
# ---------------------------------------------------------------------------

# Show workspace dependency tree
deps:
    cargo tree --workspace --depth 1

# Count lines of code (requires tokei)
loc:
    tokei rememhq-core rememhq-api rememhq-cli rememhq-mcp libremem

# Print workspace member crate sizes
sizes:
    @echo "Crate sizes (release):"
    @du -sh target/release/rememhq-api target/release/rememhq-cli 2>/dev/null || echo "Build release first: just build-release"
