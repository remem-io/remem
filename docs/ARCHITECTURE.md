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

## Crate Structure

### rememhq-core
The central library. Contains:
- **memory/** — Core types (MemoryRecord, MemoryType, request/response structs)
- **storage/** — SQLite persistence (WAL + FTS5) and vector index
- **providers/** — Cloud LLM clients (Anthropic, OpenAI) and embedding providers
- **reasoning/** — The differentiator: scoring, guided retrieval, consolidation, contradiction detection
- **config** — TOML + env var configuration

### rememhq-mcp
MCP server exposing 6 tools over stdio JSON-RPC:
`mem_store`, `mem_recall`, `mem_search`, `mem_update`, `mem_forget`, `mem_consolidate`

### rememhq-api
REST API server (Axum) mirroring the MCP tools as HTTP endpoints.
Includes bearer auth, CORS, and request tracing.

### rememhq-cli
CLI binary (`remem`) with subcommands: serve, mcp, store, recall, search, inspect, models.

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
