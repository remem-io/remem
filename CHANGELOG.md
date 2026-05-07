# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Core reasoning engine with LLM-based importance scoring, guided retrieval, consolidation, and contradiction detection
- SQLite storage backend with WAL mode, FTS5 full-text search, and knowledge graph persistence
- Brute-force vector index with cosine similarity (v0.1 — HNSW in v0.2)
- Anthropic Claude provider (Messages API)
- OpenAI provider (Chat Completions + Embeddings API)
- MCP server with 6 tools: `mem_store`, `mem_recall`, `mem_search`, `mem_update`, `mem_forget`, `mem_consolidate`
- REST API server (Axum) with bearer auth, CORS, and request tracing
- CLI (`remem`) with serve, mcp, store, recall, search, inspect subcommands
- Python SDK (async-first, Pydantic v2, httpx)
- TypeScript SDK (native fetch, zero runtime deps)
- Multi-crate Rust workspace architecture
- Cross-platform CI (Linux, macOS, Windows)
- Docker support with multi-stage builds
- Dev container configuration

### Architecture
- `rememhq-core` — storage, providers, reasoning engine
- `rememhq-mcp` — MCP server (stdio JSON-RPC)
- `rememhq-api` — REST API (Axum)
- `rememhq-cli` — CLI binary
- `sdk/python` — Python SDK
- `sdk/typescript` — TypeScript SDK

## [0.1.0] — Unreleased

Initial release. See [Added] above.

[Unreleased]: https://github.com/rememhq/remem/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/rememhq/remem/releases/tag/v0.1.0
