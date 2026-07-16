# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- `SqliteStore::apply_decay`: the importance-weighted decay multiplier could exceed `1.0` (`decay_factor + importance / 20.0`), causing `decay_score` to *increase* instead of decrease for any memory with importance >= 3 — including the default importance of `5.0` given to every new memory. In practice this meant the background decay job (`decay_factor = 0.9`) never decayed or auto-archived the majority of memories. The multiplier is now interpolated between `decay_factor` and `1.0` based on importance, so it can never exceed `1.0`.

## [0.1.5] - 2026-06-19

### Added
- Session lifecycle management: `create_session`, `end_session`, `get_session`, `list_sessions`, and `increment_session_memory_count` on `SqliteStore`, backed by the existing `sessions` table
- `SessionRecord` type exposing session ID, project, start/end timestamps, consolidation status, and memory count
- TTL expiration: `expire_ttl()` archives memories whose `ttl_days` has elapsed, computed via SQLite's `julianday()`
- Auto-save background tasks in the REST API server
- Graceful shutdown for the REST API server — listens for `Ctrl+C` and (on Unix) `SIGTERM`, draining in-flight requests before exit
- `rememhq-core::providers::factory` module centralizing reasoning/embedding provider construction with cascading fallbacks (configured provider → alternatives → `MockProvider`), replacing ~150 duplicated lines across `cli`, `api`, and `mcp` binaries
- `CDLA-Permissive-2.0` added to the `cargo-deny` allowed license list

### Changed
- Google embedding model: `text-embedding-004` → `gemini-embedding-2`
- Vector index dimension is now sized dynamically from the active embedding provider instead of a hardcoded constant
- Various FFI, CLI, and API modules simplified after extracting provider construction into `factory.rs`

### Fixed
- FFI leak in `remem_index_new` / `remem_embedder_new`: the inner C++ object is now constructed before the outer wrapper, so a constructor exception in the inner object no longer leaks an already-allocated wrapper
- Removed temporary local test scripts (`test_api.py`, `test_embed_dim.py`, `test_models.py`, `check_licenses.py`) from git tracking

## [0.1.4] - 2026-06-15

### Added
- `mem_query_knowledge` MCP tool — query the knowledge graph by subject, predicate, and/or object filters
- `mem_get_entity_context` MCP tool — retrieve all knowledge graph triples for a named entity (MCP tool count: 7 → 9)
- `rememhq-core::models` module — `ModelSpec` registry, `pull_model()` streaming download (temp-file-safe, skips already-present files), `find_model()` lookup, and `default_models_dir()` helper
- `remem models pull <id>` CLI command — downloads ONNX + vocab files for local embedding models with progress output and env-var hints
- `remem models list` CLI command — shows installed/missing status for all known models
- 19 Python SDK mock-based integration tests covering every client endpoint
- 24 TypeScript SDK mock-based integration tests covering every client method

### Changed
- `reqwest` workspace dependency: added `stream` feature (required by `pull_model` streaming download)
- Workspace `Cargo.toml`: removed unused `rmcp` dependency
- `CODEOWNERS`: corrected all paths from legacy `remem-*` to `rememhq-*` crate names
- Docker base image bumped from `rust:1.75-bookworm` to `rust:1.96-bookworm`
- `rusqlite` bumped from `0.39.0` to `0.40.0`

### Fixed
- All GitHub Actions workflows pinned to real released versions (removed phantom `@v6`/`@v7`/`@v8` references across 12 workflow files)
- `attest-build-provenance` step marked `continue-on-error: true` to tolerate transient Sigstore/Rekor infrastructure errors
- MSRV check in `nightly.yml` corrected from nonexistent `@1.100.0` to `@1.75.0`

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

[Unreleased]: https://github.com/remem-io/remem/compare/v0.1.5...HEAD
[0.1.5]: https://github.com/remem-io/remem/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/remem-io/remem/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/remem-io/remem/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/remem-io/remem/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/remem-io/remem/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/remem-io/remem/releases/tag/v0.1.0
