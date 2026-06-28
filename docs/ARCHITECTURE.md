# remem Architecture

## System Overview

remem is a reasoning memory layer for AI agents. Unlike simple vector stores, remem adds an LLM reasoning step at every key memory operation: storing, retrieving, consolidating, and detecting contradictions.

## Layer Diagram

```
┌─────────────────────────────────────────────────────┐
│                  Agent Consumers                     │
│  Claude Code · Cursor · Python agents · TS agents   │
└──────────┬──────────────────┬───────────────────────┘
           │ MCP stdio        │ REST API / SDK
┌──────────▼──────────────────▼───────────────────────┐
│              Interface Layer  (Rust)                 │
│     rememhq-mcp (stdio) · rememhq-api (Axum REST)   │
│     Python SDK (httpx)  · TypeScript SDK (fetch)    │
└──────────────────────┬──────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────┐
│          Reasoning Engine  (rememhq-core)            │
│  Consolidation · Guided Retrieval · Contradiction   │
│  Detection · Importance Scoring · Knowledge Graph   │
└──────┬──────────────────────────┬───────────────────┘
       │                          │
┌──────▼──────┐          ┌────────▼────────────────────┐
│ Cloud APIs  │          │  Storage Layer               │
│ Anthropic   │          │  SQLite + WAL (metadata)     │
│ OpenAI      │          │  Vector Index (cosine sim)   │
└─────────────┘          └─────────────────────────────┘
```

## Workspace & Crate Structure

The repository operates as a polyglot workspace with Rust at its core, a C++ engine for high-performance indexing, and SDKs spanning Python, TypeScript, and React Native.

### 1. Engine & Core Logic (`rememhq-core`)
The central library implementing all business logic, storage, and reasoning. No other crate interacts directly with the database or LLMs; they all route through `rememhq-core`.
- **`src/memory/`** — Core domain models (`MemoryRecord`, `MemoryType`, request/response structs).
- **`src/storage/`** — SQLite persistence (WAL mode + FTS5 full-text search) and vector index orchestration.
- **`src/providers/`** — Cloud LLM clients (Anthropic, OpenAI) and local ONNX models for generating reasoning outputs and embeddings.
- **`src/reasoning/`** — The reasoning engine: handles importance scoring, guided retrieval, session consolidation, and contradiction detection.
- **`src/config/`** — Unified configuration management utilizing TOML and environment variables.

### 2. High-Performance Vector Search (`libremem`)
A dedicated C++17 library powering local vector similarity search and fast embeddings.
- **`src/` & `include/`** — Implements `hnswlib` for Hierarchical Navigable Small World vector indexing and `ONNX Runtime` for local embedding models (e.g., `nomic-embed-text`).
- **Integration** — Compiled dynamically via `build.rs` in `rememhq-core` using the `cc` crate. Communication happens strictly over a zero-cost C-FFI bridge.

### 3. Server Interfaces (Transports)
Thin transport layers that wrap the `rememhq-core` engine and expose it over different protocols.
- **`rememhq-api/`** — An HTTP REST API server built with `Axum`. Mirrors core operations as HTTP endpoints, injecting bearer auth, CORS, and request tracing via `tracing-subscriber`.
- **`rememhq-mcp/`** — A Model Context Protocol (MCP) server communicating via `stdio` JSON-RPC. Heavily utilized by agent consumers (Claude Code, Cursor) and exposes tools like `mem_store`, `mem_recall`, and `mem_consolidate`.

### 4. Client Tooling (`rememhq-cli`)
The primary CLI entrypoint (`remem.exe`). 
- **Subcommands** — Provides commands such as `agent` (an interactive terminal companion), `mcp`, `serve`, `store`, `recall`, `search`, and `inspect`.

### 5. SDKs & Bindings
Client libraries for consuming the `rememhq-api` or directly embedding the core library.
- **`sdk/python/`** — Async Python SDK built atop `httpx` and `pydantic`.
- **`sdk/typescript/`** — Node/Browser-compatible TypeScript SDK using modern `fetch`.
- **`sdk/rust/`** — Rust SDK (`rememhq`) re-exporting the `rememhq-core` client.
- **`bindings/react-native/`** — Specialized bindings allowing `remem` logic to be embedded natively into mobile environments using JSI.
- **`evals/`** — Evaluation benchmarks for testing recall accuracy and latency regressions across models.

## Data Flow

### Store
```
content → embed (cloud API) → vector index + SQLite insert
        ↘ LLM scores importance (if not provided)
```

### Recall (Guided Retrieval)
```
query → embed → vector index top-50
       → fetch records from SQLite (apply filters)
       → LLM re-ranks candidates with reasoning
       → return top-k with reasoning traces
```

### Consolidate
```
session memories → LLM extracts durable facts
                → deduplicate against existing (cosine > 0.92)
                → detect contradictions with existing memories
                → store new facts + update knowledge graph
```

## Storage

### SQLite (WAL mode)
- `memories` table with FTS5 virtual table for keyword search
- `knowledge_graph` table for subject-predicate-object triples
- `sessions` table for session tracking
- Triggers keep FTS index in sync with memory changes

### Vector Index
- v0.1: brute-force cosine similarity (sufficient for ~100K memories)
- v0.2+: hnswlib C++ integration for sub-linear search
- Persisted to disk as JSON (v0.1) / binary (v0.2+)
