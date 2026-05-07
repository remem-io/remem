# Contributing to remem

Thank you for your interest in contributing to remem! We welcome contributions from the community.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Making Changes](#making-changes)
- [Pull Request Process](#pull-request-process)
- [Coding Standards](#coding-standards)
- [Architecture](#architecture)

## Code of Conduct

This project follows the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## Getting Started

1. **Fork** the repository at [github.com/rememhq/remem](https://github.com/rememhq/remem)
2. **Clone** your fork locally
3. **Create a branch** for your work (`git checkout -b feat/my-feature`)
4. **Make changes** and commit with clear messages
5. **Push** to your fork and submit a Pull Request

### Good First Issues

Look for issues labeled [`good first issue`](https://github.com/rememhq/remem/labels/good%20first%20issue) for beginner-friendly tasks.

## Development Setup

### Prerequisites

- **Rust** stable (1.75+) — install via [rustup](https://rustup.rs/)
- **Python** 3.11+ (for Python SDK development)
- **Node.js** 20+ (for TypeScript SDK development)

### Build from Source

```bash
git clone https://github.com/rememhq/remem.git
cd remem

# Build all Rust crates
cargo build --workspace

# Run tests
cargo test --workspace

# Format check
cargo fmt --all -- --check

# Lint
cargo clippy --workspace -- -D warnings
```

### Python SDK Development

```bash
cd sdk/python
pip install -e ".[dev]"
pytest tests/ -v
```

### TypeScript SDK Development

```bash
cd sdk/typescript
npm install
npm run build
```

## Making Changes

### Where to Contribute

| Area | Directory | Language |
|---|---|---|
| Core reasoning engine | `rememhq-core/src/reasoning/` | Rust |
| Storage layer | `rememhq-core/src/storage/` | Rust |
| Cloud providers | `rememhq-core/src/providers/` | Rust |
| MCP server | `rememhq-mcp/src/` | Rust |
| REST API | `rememhq-api/src/` | Rust |
| CLI | `rememhq-cli/src/` | Rust |
| Python SDK | `sdk/python/` | Python |
| TypeScript SDK | `sdk/typescript/` | TypeScript |
| C++ core (v0.2+) | `libremem/` | C++ |
| Documentation | `docs/` | Markdown |

### Commit Messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(core): add knowledge graph triple extraction
fix(mcp): handle malformed JSON-RPC requests
docs: update ARCHITECTURE.md with consolidation flow
test(core): add vector index persistence tests
chore(ci): add Windows arm64 to build matrix
```

## Pull Request Process

1. **Open an issue first** for large changes to discuss the approach
2. **Keep PRs focused** — one feature or fix per PR
3. **Update documentation** if your changes affect the API or behavior
4. **Add tests** for new functionality
5. **Ensure CI passes** — all checks must be green before merge
6. **Request review** from a maintainer

### PR Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] New code has tests
- [ ] Documentation updated (if applicable)
- [ ] Commit messages follow Conventional Commits

## Coding Standards

### Rust

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `anyhow::Result` for application-level errors
- Use `thiserror` for library-level error types
- Prefer `async`/`await` over blocking I/O
- All public types must have doc comments
- No `unsafe` code without a safety comment and maintainer approval

### Python

- Follow PEP 8 and use type hints everywhere
- Use Pydantic v2 for data models
- Use `async`/`await` for all I/O operations

### TypeScript

- Strict mode enabled
- Use native `fetch` (no axios/node-fetch)
- Export all public types

## Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full system design. Key principles:

- **Reasoning first** — every operation goes through an LLM reasoning step
- **Cloud-native, local-ready** — cloud APIs in v0.1, local models in v0.2+
- **Multi-interface** — MCP, REST, and native SDKs all share the same core
- **Memory taxonomy** — facts, procedures, preferences, and decisions are distinct types

## Questions?

- Open a [Discussion](https://github.com/rememhq/remem/discussions) for questions
- File an [Issue](https://github.com/rememhq/remem/issues) for bugs or feature requests
- Tag `@thrive-spectrexq` for maintainer attention

---

Thank you for helping make agent memory better! 🧠
