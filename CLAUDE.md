# CLAUDE.md — Agent Onboarding and Guidelines

This file provides system context, build/test commands, and code style rules for `remem`.

## Project Overview
`remem` is a **reasoning memory layer for AI agents** — a persistent, queryable memory system that uses LLM-powered reasoning for importance scoring, contradiction detection, knowledge graph construction, and session consolidation.

## Commands

### Build & Run
- **Build workspace:** `cargo build --workspace`
- **Run tests:** `cargo test --workspace`
- **Run API server:** `cargo run -p rememhq-api -- --project default`
- **Run MCP server:** `cargo run -p rememhq-mcp`
- **Run CLI tool:** `cargo run -p rememhq-cli -- --help`
- **Init agent config:** `cargo run -p rememhq-cli -- init <consumer>` (claude-code, codex, cursor, copilot, gemini-cli, opencode, all)

### Formatting & Linting
- **Check formatting:** `cargo fmt --all -- --check`
- **Format code:** `cargo fmt`
- **Lint check:** `cargo clippy --workspace --all-targets -- -D warnings`

### SDKs
- **Python SDK:** `cd sdk/python && pip install -e ".[dev]" && pytest tests/`
- **TS SDK:** `cd sdk/typescript && npm install && npm run build`

## Coding Conventions

### Rust (rememhq-core, rememhq-api, rememhq-mcp)
- **Edition:** 2021
- **Async I/O:** Always use `async/await` with Tokio.
- **Error Handling:** Use `anyhow::Result` for application code; `thiserror` for library error types.
- **Naming:** `snake_case` for functions/variables, `PascalCase` for types, `SCREAMING_SNAKE` for constants.
- **Lints:** Strict formatting (`cargo fmt`) and clippy compliance (no warnings allowed, CI runs with `-D warnings`).
- **C++ FFI:** C++ vector FFI is accessed exclusively via the FFI bridge in `rememhq-core/src/storage/vector.rs::remem_ffi`. Use opaque `*mut c_void` handles and wrap every FFI call in try-catch on the C++ side.

### C++ (libremem)
- **Standard:** C++17
- **Exception Safety:** All exported FFI functions must catch exceptions internally and return zero/null on failure.
- **Headers:** Public API in `libremem/src/ffi/remem.h`.
