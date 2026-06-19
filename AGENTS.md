# AGENTS.md — AI Coding Agent Instructions

> This file provides context and rules for AI coding agents (Claude, Gemini, Codex, Copilot, Cursor, etc.) working on the `remem` codebase.

## Repository Overview

`remem` is a **reasoning memory layer for AI agents** — a persistent, queryable memory system that uses LLM-powered reasoning for importance scoring, contradiction detection, knowledge graph construction, and session consolidation.

### Architecture

```
remem (workspace root)
├── rememhq-core/       # Core library: config, memory types, providers, reasoning, storage
├── rememhq-api/        # REST API server (Axum)
├── rememhq-cli/        # Command-line interface
├── rememhq-mcp/        # Model Context Protocol server
├── libremem/           # C++ core: HNSW vector index + ONNX embedding engine (FFI bridge)
├── sdk/
│   ├── python/         # Python SDK (httpx + pydantic)
│   ├── typescript/     # TypeScript SDK
│   └── rust/           # Rust SDK (re-export of rememhq-core, planned)
├── evals/              # Evaluation benchmarks
├── docs/               # Architecture, provider, and reasoning documentation
└── .github/            # CI/CD workflows, issue templates, dependabot
```

### Key Abstractions

| Trait / Interface | Location | Purpose |
|---|---|---|
| `Provider` | `rememhq-core/src/providers/mod.rs` | Cloud LLM completion (Anthropic, OpenAI, Google, Mock) |
| `EmbeddingProvider` | `rememhq-core/src/providers/mod.rs` | Embedding generation (OpenAI, Google, Local ONNX, Mock) |
| `MemoryStore` | `rememhq-core/src/storage/mod.rs` | Persistent storage (SQLite) |
| `VectorIndex` | `rememhq-core/src/storage/vector.rs` | Vector similarity search (HNSW via C++ FFI) |
| `ReasoningEngine` | `rememhq-core/src/reasoning/mod.rs` | Orchestrates store, recall, consolidate |

## Build & Run

### Prerequisites
- **Rust** 1.75+ (see `rust-toolchain.toml`)
- **C++ compiler** with C++17 support (for `libremem`)
- **Python** 3.11+ (for Python SDK)
- **Node.js** 20+ (for TypeScript SDK)

### Commands
```bash
# Build entire workspace (includes C++ compilation via build.rs)
cargo build --workspace

# Run tests
cargo test --workspace

# Run the API server
cargo run -p rememhq-api -- --project myproject

# Run the MCP server
cargo run -p rememhq-mcp

# Run the CLI
cargo run -p rememhq-cli -- --help

# Initialize MCP config for an agent consumer
# Supported: claude-code, codex, cursor, copilot, gemini-cli, opencode, all
remem init cursor --project my-project
remem init all --project my-project

# Python SDK
cd sdk/python && pip install -e ".[dev]" && pytest tests/ -v

# TypeScript SDK
cd sdk/typescript && npm install && npm run build
```

## Coding Conventions

### Rust
- **Edition**: 2021
- **Format**: Always run `cargo fmt` — CI enforces `--check`
- **Clippy**: CI runs with `-Dwarnings` — no clippy warnings allowed
- **Error handling**: Use `anyhow::Result` for application code; `thiserror` for library error types
- **Async**: All I/O-bound operations use `async/await` with Tokio
- **Naming**: snake_case for functions/variables, PascalCase for types, SCREAMING_SNAKE for constants
- **FFI**: All C++ interop goes through `rememhq-core/src/storage/vector.rs::remem_ffi` module. Use opaque `*mut c_void` handles. **Every FFI call must be wrapped in try-catch on the C++ side.**

### C++ (`libremem`)
- **Standard**: C++17
- **Build**: Compiled via `cc` crate in `rememhq-core/build.rs` — NOT standalone CMake
- **FFI safety**: All exported functions must catch exceptions and return null/zero on failure
- **Headers**: Public API in `libremem/src/ffi/remem.h`

### Python SDK
- **Style**: PEP 8, enforced by `ruff`
- **Types**: Full type annotations with Pydantic models
- **Testing**: `pytest` + `pytest-asyncio`

### TypeScript SDK
- **Target**: ES2022, Node 20+
- **Strict mode**: `"strict": true` in tsconfig

## Testing Requirements

- **All new Rust code** must include unit tests in the same file (`#[cfg(test)] mod tests`)
- **Integration tests** go in `rememhq-core/tests/`
- **SDK changes** must include corresponding test updates
- **C++ FFI changes** must be verified with `cargo build --workspace` at minimum

## Commit & PR Conventions

### Commit Messages
Follow [Conventional Commits](https://www.conventionalcommits.org/):
```
feat(core): add entity resolution during consolidation
fix(api): handle missing auth header gracefully
chore(deps): bump tokio to 1.38
docs(sdk): update Python quickstart examples
test(core): add sqlite store archival tests
```

### Pull Requests
- Fill out the PR template (`.github/PULL_REQUEST_TEMPLATE.md`)
- All CI checks must pass
- At least one approval from `@thrive-spectrexq`

## Architecture Boundaries

- **`rememhq-core`** is the only crate that touches SQLite, the vector index, or cloud providers. All other crates depend on it.
- **`rememhq-api`** and **`rememhq-mcp`** are thin transport layers — business logic belongs in `rememhq-core`.
- **`libremem`** C++ code is accessed **only** through the FFI bridge in `rememhq-core/src/storage/vector.rs`. No other crate should import C++ symbols directly.
- **SDKs** are pure HTTP clients — they do not embed any Rust code.

## Environment Variables

| Variable | Purpose | Default |
|---|---|---|
| `REMEM_PROVIDER` | Default provider for reasoning + embeddings | `anthropic` |
| `REMEM_REASONING_PROVIDER` | Override reasoning provider only | Falls back to `REMEM_PROVIDER` |
| `REMEM_EMBEDDING_PROVIDER` | Override embedding provider only | Falls back to `REMEM_PROVIDER` |
| `REMEM_LOCAL_MODEL_PATH` | Path to ONNX model for local embeddings | `models/nomic-embed-text.onnx` |
| `REMEM_DATA_DIR` | Root data directory | `~/.remem` |
| `REMEM_API_KEY` | API key for authenticating requests | None (auth disabled) |
| `GOOGLE_API_KEY` | Google Gemini API key | None |
| `OPENAI_API_KEY` | OpenAI API key | None |
| `ANTHROPIC_API_KEY` | Anthropic API key | None |
