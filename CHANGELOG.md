# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.13] - 2026-07-23

### Added
- **Reciprocal Rank Fusion (RRF) Hybrid Search**: Combined HNSW vector similarity search with SQLite FTS5 BM25 keyword matching (`RRF = 1/(60+r_vec) + 1/(60+r_bm25)`).
- **TTL Auto-Archiving Maintenance**: Automatic background archiving of memories whose `ttl_days` period has elapsed during decay processing.
- **SDK Expansion**: Added `store_batch` (Python) and `storeBatch` (TypeScript) client methods.
- **Provider Resolution & Multi-Provider Seamless Support**: Flexible provider alias matching (`gemini`, `google`, `claude`, `anthropic`, `openai`) across all environment configuration chains.

### Fixed
- Fixed soft-deleted node filtering and active element count calculation in HNSW vector index (`libremem`).
- Fixed sub-millisecond clock evaluation and deterministic `created_at` handling in TTL auto-archiving tests.
- Fixed `clippy::min_max` warning in retrieval candidate calculation.

### Added
- **Evaluation Harness Overhaul**: Complete rewrite of the evaluation and benchmarking suite in `evals/benchmark.py`.
  - Added configurable pass/fail thresholds in `evals/thresholds.json`.
  - Fixed contradiction detection evaluation to properly measure conflict surfacing and resolution.
  - Added structured JSON reporting (`--output json`, `--output-file`).
  - Added deterministic mock mode (`--seed`).
  - Added run-over-run regression comparison (`--baseline`).
  - Expanded test coverage to evaluate `ForgetMode` (delete/archive), tag filtering, Unicode/long content edge cases, and session consolidation quality.
  - Updated `evals/README.md` with comprehensive metrics reference and GitHub Actions workflow examples.

### Fixed
- Derived `Debug` on `ErrorResponse` and `StoreResponse` DTOs in `rememhq-api/src/routes/memories.rs` to fix `Result::unwrap()` debug formatting errors under `cargo llvm-cov`.
- Allowed `clippy::await_holding_lock` in API unit test for process-wide env var synchronization across await points.
- Added `--ignore-run-fail` to `cargo-llvm-cov` steps in CI workflows.

### Added (earlier)
- `ReActLoop` now actually performs context compaction once its message history exceeds 20 messages, using the existing `compact_context` reasoning primitive. Previously this was a stub: the loop logged that context was large and left a comment ("Compact logic could be hooked here") but never did anything, so long-running agent loops accumulated unbounded context. The system prompt and original task message are always preserved; everything since then is summarized into a single message once the threshold is hit. A failed compaction attempt is non-fatal — the loop just continues and tries again once more history builds up.

### Security
- **`js-yaml` moderate-severity DoS (quadratic complexity in merge-key handling), transitive via `@istanbuljs/load-nyc-config`** in `bindings/react-native`. Bumped the nested resolution from 3.14.2 to 3.15.0 (the patched version), within the existing `^3.13.1` range already required by that package — no code changes needed. Dev/test-tooling dependency only, not part of any published artifact.
  - Note: `bindings/react-native` also carries a moderate `uuid` advisory (transitive via `xcode`, a dependency of `@expo/config-plugins`), but `xcode`'s current release still pins `uuid@^7.0.3`, so there's no non-breaking upgrade path available yet. Left as-is pending an upstream fix; worth revisiting when `xcode` publishes a new major version.
- **Unbounded `limit`/`offset` on `/v1/memories/recall` and `/v1/memories/search`.** These endpoints computed `offset + limit` directly from query-string input with no upper bound, then passed it straight through to the vector index search (and downstream, an FFI call into the native HNSW library). A single request with an enormous `limit` — no auth required if `REMEM_API_KEY` isn't set — could force a huge search/allocation there, and extreme values could overflow the `usize` addition. Both endpoints now reject `limit + offset` above 1000 with a 400.
- **Rate-limit bypass via spoofed `X-Forwarded-For`.** `rememhq-api`'s rate limiter keyed requests directly on the client-supplied `X-Forwarded-For` header, which any caller can set to an arbitrary value. Sending a different value on every request bypassed the limiter entirely, since each "new" value got its own fresh bucket. By default, the limiter now keys on the actual TCP peer address (via `ConnectInfo`, which a caller cannot forge) instead. Operators genuinely running behind a proxy/load balancer that overwrites the header can opt back into the old behavior with `REMEM_TRUST_PROXY_HEADERS=true`.
- **Timing side-channel in API key comparison.** `check_auth` compared the provided `Authorization` bearer token against `REMEM_API_KEY` with `!=`, which short-circuits on the first mismatched byte. An attacker measuring response latency over enough requests could in principle recover the key byte by byte. Replaced with a constant-time comparison that always walks every byte.
- **Unbounded `limit` on MCP tool calls (`mem_recall`, `mem_search`, `mem_list_memories`, `mem_list_sessions`, `mem_get_project_context`).** Same class of issue as the REST `limit`/`offset` fix above, but on the MCP surface: `limit` from tool-call arguments went straight through to the vector index search / FFI call with no upper bound — and here the caller is an LLM agent that may be acting on untrusted content (prompt injection), not a human typing a URL. Added a shared `clamp_limit` helper (cap of 1000, matching the REST API's `MAX_FETCH_LIMIT`) and applied it at all five call sites.

### Fixed
- `compact_context`: `CompactionReport.compressed_length` was measured on the raw, untrimmed provider response, while `compressed_context` stored the trimmed version — so whenever the LLM padded its output with whitespace (common), the two disagreed. Both fields are surfaced verbatim to end users (REST API `/v1/context/compact` response, MCP `compact_context` tool text), so this produced a visibly wrong character count next to the actual compacted text.
- `AgentHarness::chat_with_validation`: the accepted assistant message was never appended to the caller's `messages` history on success (only on the retry/failure paths were messages pushed) — despite the method taking `&mut Vec<ChatMessage>` specifically to keep that history in sync. A caller reusing `messages` for a follow-up turn would silently lose the assistant's prior response.
- `SchemaValidator::validate`: when the top-level JSON value wasn't an object, the error always reported `actual: "other"` regardless of what it actually was (array, string, number, etc.), instead of the real type name used everywhere else in the same function.
- `SqliteStore::apply_decay`: the importance-weighted decay multiplier could exceed `1.0` (`decay_factor + importance / 20.0`), causing `decay_score` to *increase* instead of decrease for any memory with importance >= 3 — including the default importance of `5.0` given to every new memory. In practice this meant the background decay job (`decay_factor = 0.9`) never decayed or auto-archived the majority of memories. The multiplier is now interpolated between `decay_factor` and `1.0` based on importance, so it can never exceed `1.0`.
- `score_importance`: required the LLM's response to parse as an exact bare number, so any deviation from that (`"Score: 7"`, `"7/10"`, `"I'd say 7."`) silently fell back to a default of `5.0` with no indication the model's actual rating was discarded. Now falls back to extracting the first numeric token in the response before defaulting, and logs a warning when no number can be found at all.
- `llm_rerank` (guided retrieval): the LLM's `SELECTED [N] | reasoning` output was parsed without deduplicating `N`. If the model listed the same candidate index twice (e.g. under two different rationales, which happens occasionally with weaker/faster models), that memory occupied two slots in the result set, silently crowding out a genuinely distinct candidate within `limit`. Selections are now deduplicated by index.
- `extract_facts` (consolidation): a `TRIPLE` line in the LLM's response was attached to whichever `FACT` line came *after* it, rather than the one it actually describes. Both the prompt's general instruction (add a triple right after the fact it describes) and its own procedure-chaining example (the triple's subject matches the *preceding* fact's content, e.g. `"To deploy, first run build"`) confirm a triple belongs to the fact before it. This wasn't just a bookkeeping error: `knowledge_graph.memory_id` has `ON DELETE CASCADE`, so a misattributed triple was tied to the wrong memory's lifecycle — it could vanish when an unrelated memory was deleted, or fail to vanish when the memory it actually describes was. Triples are now attached to the preceding fact; a stray `TRIPLE` with no preceding fact is dropped rather than misattributed to an unrelated later one.
- `rememhq-cli`'s interactive agent startup banner truncated the version, provider, model, and project directory strings by raw byte index (`&s[0..n]`), which panics with "byte index n is not a char boundary" if the cut lands in the middle of a multi-byte UTF-8 character. Most plausible in practice for the project directory path, which routinely contains non-ASCII characters (accents, CJK, etc.) in real usernames or folder names — the CLI could crash on startup for those users depending on exact path length. Replaced with character-boundary-safe truncation helpers.
- `generate_session_summary`: when the LLM's response didn't parse as the expected JSON, this silently fell back to `Ok(SummaryOutput { summary: "Failed to parse session summary.", .. })` — no error, no log. The caller (`compress_session_transcript`) only logs a warning on `Err`, so this placeholder was persisted into `session_summaries` as if it were real, with nothing anywhere indicating the parse had failed. Worse, that placeholder is exactly what `mem_get_project_context` surfaces to future sessions under "Recent Sessions Timeline". Parse failures now propagate as `Err` (including a truncated snippet of the raw response), so the existing warning-log-and-skip-insert path in the caller actually runs instead of silently persisting garbage.
- `extract_facts` (consolidation): a type-inference regression from the `TRIPLE`-attribution fix above broke the build (`let mut facts = Vec::new();` couldn't be inferred once a field access preceded the `.push()` call that used to resolve it). Fixed by annotating the type explicitly.
- `truncate_chars_back` (CLI agent banner): `truncate_chars_back(s, 0)` returned the *full* string instead of an empty one. `char_indices().nth(skip)` returns `None` exactly when `max_chars == 0` (asking to keep zero characters), but the fallback incorrectly returned `s` — the correct behavior for `truncate_chars_front`'s analogous `None` case (which means "shorter than requested"), but wrong here. Not reachable from the actual call site today (`max_chars` is hardcoded to `38`), but a real bug in a general-purpose utility function, caught by the same regression test that verified the earlier panic fix.
- Ran `cargo fmt` across every file touched in this consolidation-and-cleanup pass; several had drifted from the project's formatting rules (I had no working `rustfmt` for most of this work).

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
