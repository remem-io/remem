# LLM.md — Context for AI Assistants

> Quick-reference context document for LLMs working on remem.

## What is remem?

A **reasoning memory layer for AI agents**. It enhances AI assistants with an intelligent memory layer that enables personalized interactions — it remembers user preferences, adapts to individual needs, and continuously learns over time. Backed by a SQLite store and an HNSW vector index implemented in C++, remem stores memories with importance scoring, retrieves them with LLM-guided reasoning, detects contradictions between old and new information, and consolidates session data into durable knowledge.

## Supported Agent Consumers

remem integrates with the following AI coding assistants via MCP (stdio):

| Consumer | Config Location | Setup Command |
|---|---|---|
| **Claude Code** | `.claude/config.json` | `remem init claude-code` |
| **Codex** | `.codex/config.json` | `remem init codex` |
| **Cursor** | `.cursor/mcp.json` | `remem init cursor` |
| **GitHub Copilot** | `.github/copilot/mcp.json` | `remem init copilot` |
| **Gemini CLI** | `.gemini/settings.json` | `remem init gemini-cli` |
| **OpenCode** | `.opencode/config.json` | `remem init opencode` |

Use `remem init all` to generate configs for every consumer at once.

## Tech Stack

- **Language**: Rust (core engine), C++17 (vector index + ONNX embeddings), Python & TypeScript (SDKs)
- **Async Runtime**: Tokio
- **Web Framework**: Axum (REST API)
- **Storage**: SQLite (rusqlite, bundled), HNSW (hnswlib via FFI)
- **LLM Providers**: Anthropic Claude, OpenAI GPT, Google Gemini, Mock (offline), Local ONNX
- **Build**: Cargo workspace + `cc` crate for C++ compilation

## Core Flow

```
User → store("I prefer dark mode")
  → LLM scores importance → embed text → SQLite + HNSW index

User → recall("What are Alice's preferences?")
  → embed query → HNSW search → LLM re-ranks → return results

System → consolidate(session_id)
  → extract facts → detect contradictions → build knowledge graph
  → archive superseded memories → store procedural steps
```

## Key Types

```rust
MemoryRecord { id, content, embedding, importance, tags, memory_type, ... }
MemoryResult  { id, content, importance, similarity, reasoning, ... }
MemoryType    { Fact, Procedure, Preference, Decision }
```

## FFI Boundary (C++ ↔ Rust)

All C++ interop lives in `rememhq-core/src/storage/vector.rs::remem_ffi`:
- `remem_index_*` — HNSW vector index operations
- `remem_embedder_*` — ONNX embedding engine operations
- C++ side: `libremem/src/ffi/remem.h` and `remem.cpp`
- **Rule**: Every C++ FFI function wraps logic in `try-catch` to prevent Rust panics from foreign exceptions

## Common Pitfalls

1. **Windows PowerShell**: Use `;` not `&&` to chain commands
2. **Cargo.lock**: Tracked in git (binary workspace). Don't add to `.gitignore`
3. **C++ build**: `build.rs` compiles C++ — changes to `libremem/src/` trigger recompilation
4. **Provider selection**: `REMEM_REASONING_PROVIDER` and `REMEM_EMBEDDING_PROVIDER` override `REMEM_PROVIDER`
5. **Unicode in Python**: Set `PYTHONIOENCODING=utf-8` on Windows for emoji-heavy output
6. **Enum in extern block**: Rust enums cannot be defined inside `extern "C"` blocks — define them outside
