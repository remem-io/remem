<div align="center">
  <h1>remem</h1>
  <p><strong>The reasoning memory layer for AI agents.</strong></p>
  <p>
    <a href="https://github.com/remem-io/remem/actions"><img src="https://github.com/remem-io/remem/workflows/CI/badge.svg" alt="CI Status" /></a>
    <a href="https://github.com/remem-io/remem/releases"><img src="https://img.shields.io/github/v/release/remem-io/remem" alt="Release" /></a>
    <a href="https://apache.org/licenses/LICENSE-2.0"><img src="https://img.shields.io/badge/License-Apache%202.0-blue.svg" alt="License" /></a>
  </p>
</div>

---

> **⚠️ In Development** — remem is evolving rapidly. Not yet recommended for mission-critical production workloads.

remem provides agents with **persistent, reasoned memory** that spans across sessions. Unlike traditional vector stores that rely solely on semantic similarity, remem incorporates an LLM reasoning layer to distinguish between what is semantically close and what is actually useful for solving problems.

## Architecture

```text
┌─────────────────────────────────────────────────────┐
│                  Agent Consumers                    │
│  Claude Code · Cursor · Codex · Python/TS agents    │
└──────────┬──────────────────┬───────────────────────┘
           │ MCP stdio        │ REST API / SDK
┌──────────▼──────────────────▼───────────────────────┐
│              Interface Layer (Rust)                 │
│     rememhq-mcp (stdio) · rememhq-api (Axum REST)   │
│     Python SDK (httpx)  · TypeScript SDK (fetch)    │
└──────────────────────┬──────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────┐
│          Reasoning Engine (rememhq-core)            │
│  Consolidation · Guided Retrieval · Contradiction   │
│  Detection · Importance Scoring · Knowledge Graph   │
└──────┬──────────────────────────┬───────────────────┘
       │                          │
┌──────▼──────┐          ┌────────▼────────────────────┐
│ OpenAI      │          │  Storage Layer              │
│ Anthropic   │          │  SQLite + WAL (metadata)    │
│ Gemini      │          │  Vector Index (HNSW)        │
└─────────────┘          └─────────────────────────────┘
```

## Why remem?

Traditional vector stores often suffer from "confident recall of irrelevant context." They return what is semantically *nearest*, not what is actually *useful*. remem bridges this gap with reasoning-powered retrieval that understands context, importance, and domain-specific relevance.

| Feature | Naive Vector Store | remem |
| :--- | :--- | :--- |
| **Store** | `embed` + `insert` | `embed` + `insert` + **LLM Importance Scoring** |
| **Recall** | top-k by cosine similarity | top-50 cosine → **LLM Re-ranking** → top-8 with **Reasoning Trace** |
| **Consolidation** | — | **LLM Fact Extraction** from raw interaction logs |
| **Contradictions** | — | **LLM Conflict Detection** between old and new facts |
| **Decay** | Time-based (linear) | **Importance-Weighted Decay**; critical facts persist longer |

## 🚀 Quickstart

### Model Context Protocol (MCP) — Claude Code / Cursor / Codex

remem is designed to work seamlessly with MCP-compliant environments. Add the following to your configuration:

```json
{
  "mcpServers": {
    "remem": {
      "command": "rememhq",
      "args": ["mcp", "--project", "my-project"]
    }
  }
}
```

### Python SDK

```bash
pip install rememhq
```

```python
from rememhq import Memory

m = Memory(project="my-agent", reasoning_model="claude-sonnet-4-6")

# Store a durable preference
await m.store("The production database is PostgreSQL 15 on RDS", tags=["infra"])

# Recall with reasoning
results = await m.recall("what database are we using?")
for r in results:
    print(f"Content: {r.content}")
    print(f"Reasoning: {r.reasoning}")
```

### TypeScript SDK

```bash
npm install @rememhq/sdk
```

```typescript
import { Memory } from "@rememhq/sdk";

const m = new Memory({ project: "my-agent", reasoningModel: "gpt-5.3" });
await m.store("This repository uses trunk-based development", { tags: ["workflow"] });

const results = await m.recall("how do we manage branches?");
```

## ⚙️ Usage Commands

### Local Development

```bash
# Build the entire workspace
cargo build --workspace

# Run all tests
cargo test --workspace

# Check formatting
cargo fmt --all -- --check

# Format code
cargo fmt

# Lint (clippy) — must pass with no warnings
cargo clippy --workspace --all-targets -- -D warnings
```

### Running Services

```bash
# Start the API server (REST interface)
cargo run -p rememhq-api -- --project default

# Start the MCP server (stdio interface for Claude Code, Cursor, etc.)
cargo run -p rememhq-mcp

# Run the CLI tool
cargo run -p rememhq-cli -- --help
```

### SDKs

```bash
# Python SDK
cd sdk/python && pip install -e ".[dev]" && pytest tests/

# TypeScript SDK
cd sdk/typescript && npm install && npm run build
```

## ⚙️ How it Works

1.  **Guided Retrieval**: When you query remem, it first retrieves the top 50 candidates using cosine similarity on the vector index. These candidates are then passed to an LLM (e.g., Claude 4.6 or GPT-4) along with your query. The LLM reason about each candidate and returns the top 8 most relevant results, along with a trace of its reasoning.
2.  **Session Consolidation**: At the end of a session, remem can ingest the entire interaction log. An LLM extracts durable, high-signal facts, scores their importance, and identifies relationships, building a structured knowledge base out of raw interaction data.
3.  **Knowledge Graph & Contradiction Detection**: Facts are stored as structured nodes and edges (triples) in a knowledge graph. When new information is added that conflicts with existing knowledge, the LLM flags the contradiction, allowing you to resolve it manually or apply custom rules.
4.  **Local First**: Using `libremem`, a custom C++ engine, remem supports local HNSW indexing and BERT-compatible tokenization for privacy-first, offline embedding generation.

## 🤝 Contributing

We welcome contributions! Whether you're fixing a bug, improving the reasoning prompts, or adding a new provider, please check out our [CONTRIBUTING.md](CONTRIBUTING.md).

1.  Clone the repo: `git clone https://github.com/remem-io/remem`
2.  Build: `cargo build`
3.  Test: `cargo test --workspace`

## License

remem is licensed under the **Apache License 2.0**. See [LICENSE](LICENSE) for details.
