<div align="center">
  <h1>remem</h1>
  <p><strong>Give your AI assistant a memory that learns, adapts and reasons.</strong></p>
  <p>
    <a href="https://github.com/remem-io/remem/actions"><img src="https://github.com/remem-io/remem/workflows/CI/badge.svg" alt="CI Status" /></a>
    <a href="https://github.com/remem-io/remem/releases"><img src="https://img.shields.io/github/v/release/remem-io/remem" alt="Release" /></a>
    <a href="https://apache.org/licenses/LICENSE-2.0"><img src="https://img.shields.io/badge/License-Apache%202.0-blue.svg" alt="License" /></a>
    <a href="https://badge.fury.io/py/rememhq"><img src="https://badge.fury.io/py/rememhq.svg" alt="PyPI version" /></a>
    <a href="https://badge.fury.io/js/@rememhq%2Fsdk"><img src="https://badge.fury.io/js/@rememhq%2Fsdk.svg" alt="npm version" /></a>
    <a href="https://crates.io/crates/rememhq-core"><img src="https://img.shields.io/crates/v/rememhq-core.svg" alt="Crates.io" /></a>
  </p>
</div>

---

remem provides agents with **persistent, reasoned memory** that spans across sessions. It enhances AI assistants and agents with an **intelligent memory layer** that enables truly personalized AI interactions — it **remembers user preferences**, **adapts to individual needs**, and **continuously learns over time**, turning stateless AI tools into persistent, context-aware partners.

Unlike traditional vector stores that rely solely on semantic similarity, remem incorporates an LLM reasoning layer to distinguish between what is semantically close and what is actually useful for solving problems. Whether you're using Claude Code, Codex, Cursor, Copilot, Antigravity CLI, or OpenCode, remem gives your AI a durable, cross-session memory that grows smarter with every interaction.

## Architecture

```text
┌──────────────────────────────────────────────────────────────────────┐
│                        Agent Consumers                               │
│  Claude Code · Codex · Cursor · Copilot · Antigravity CLI · OpenCode │
│  Python agents · TypeScript agents · Any MCP-compatible client       │
└──────────┬──────────────────────┬────────────────────────────────────┘
           │ MCP stdio            │ REST API / SDK
┌──────────▼──────────────────────▼────────────────────────────────────┐
│                    Interface Layer (Rust)                            │
│       rememhq-mcp (stdio) · rememhq-api (Axum REST)                  │
│       Python SDK (httpx)  · TypeScript SDK (fetch)                   │
└────────────────────────────┬─────────────────────────────────────────┘
                             │
┌────────────────────────────▼─────────────────────────────────────────┐
│                Reasoning Engine (rememhq-core)                       │
│    Consolidation · Guided Retrieval · Contradiction Detection        │
│    Importance Scoring · Knowledge Graph · Preference Learning        │
└──────────┬─────────────────────────────┬──────────────────────────── ┘
           │                             │
┌──────────▼──────────┐       ┌──────────▼────────────────────────────┐
│ LLM Providers       │       │  Storage Layer                        │
│ OpenAI · Anthropic  │       │  SQLite + WAL (metadata)              │
│ Gemini · Local ONNX │       │  Vector Index (HNSW via libremem)     │
└─────────────────────┘       └───────────────────────────────────────┘
```

### Supported Agent Consumers

remem integrates with the leading AI coding assistants and agent frameworks. Each consumer connects through either the **MCP stdio protocol** or the **REST API / SDK**, giving your AI tools a shared, persistent memory across sessions.

| Consumer | Integration | How It Connects |
| :--- | :--- | :--- |
| **[Claude Code](https://docs.anthropic.com/en/docs/claude-code)** | MCP (stdio) | Native MCP support — add remem as an MCP server in your project config |
| **[Codex](https://github.com/openai/codex)** | MCP (stdio) | Connects via MCP server configuration, enabling persistent context across coding sessions |
| **[Cursor](https://cursor.com)** | MCP (stdio) | Add remem to Cursor's MCP settings for cross-session memory in your IDE |
| **[GitHub Copilot](https://github.com/features/copilot)** | MCP (stdio) | MCP server integration provides durable project context alongside Copilot suggestions |
| **[Antigravity CLI](https://github.com/google-antigravity/antigravity-cli)** | MCP (stdio) | Configure remem as an MCP tool server for Antigravity CLI agents |
| **[OpenCode](https://github.com/nichochar/opencode)** | MCP (stdio) | MCP-compatible — works out of the box with remem's stdio transport |
| **[Aider](https://aider.chat)** | MCP (stdio) | Auto-configured via `remem init aider` for cross-session architecture reasoning |
| **[Windsurf](https://codeium.com/windsurf)** | MCP (stdio) | Native MCP support for Windsurf workspaces |
| **[Cline](https://github.com/cline/cline)** | MCP (stdio) | Auto-injects memory limits and reasoning tools for Cline agents |
| **Python agents** | REST API / Python SDK | `pip install rememhq` — use `Memory.store()` and `Memory.recall()` in any async Python agent |
| **TypeScript agents** | REST API / TypeScript SDK | `npm install @rememhq/sdk` — typed client for Node.js and Deno agents |
| **Any MCP client** | MCP (stdio) | Any tool implementing the [Model Context Protocol](https://modelcontextprotocol.io) works with remem |

## Why remem?

- **Remembers preferences**: Coding style, tool choices, architecture decisions — stored once, recalled every time.
- **Adapts to you**: The more you interact, the better remem understands your project context and working patterns.
- **Learns continuously**: Session consolidation extracts durable knowledge from every interaction, building an ever-growing understanding of your codebase and workflows.
- **Reasons about relevance**: Unlike naive vector search, remem uses LLM reasoning to return what is actually *useful*, not just what is semantically *nearest*.

Traditional vector stores suffer from "confident recall of irrelevant context." remem bridges this gap with reasoning-powered retrieval that understands context, importance, and domain-specific relevance.

| Feature | Naive Vector Store | remem |
| :--- | :--- | :--- |
| **Store** | `embed` + `insert` | `embed` + `insert` + **LLM Importance Scoring** |
| **Recall** | top-k by cosine similarity | top-50 cosine → **LLM Re-ranking** → top-8 with **Reasoning Trace** |
| **Consolidation** | — | **LLM Fact Extraction** from raw interaction logs |
| **Contradictions** | — | **LLM Conflict Detection** between old and new facts |
| **Decay** | Time-based (linear) | **Importance-Weighted Decay**; critical facts persist longer |

## Installation

### 1. Download Pre-built Binary
Download the latest executable for your platform (Linux, macOS, Windows) from [GitHub Releases](https://github.com/remem-io/remem/releases):

```bash
# Verify installation
remem --help
```

Or build directly via Cargo:
```bash
cargo install --path rememhq-cli
```

### 2. Install SDKs

```bash
# Python SDK
pip install rememhq

# TypeScript SDK
npm install @rememhq/sdk
```

## Configuration & Environment Variables

`remem` seamlessly supports **Google Gemini**, **Anthropic Claude**, **OpenAI**, and **Local ONNX** models out of the box without code modifications. Set your preferred provider in your environment:

| Environment Variable | Description | Example |
| :--- | :--- | :--- |
| `REMEM_PROVIDER` | AI Provider choice (`gemini`, `claude`, `openai`, `local`, `mock`) | `export REMEM_PROVIDER=gemini` |
| `GOOGLE_API_KEY` | Google Gemini API key | `export GOOGLE_API_KEY="AIzaSy..."` |
| `ANTHROPIC_API_KEY` | Anthropic Claude API key | `export ANTHROPIC_API_KEY="sk-ant..."` |
| `OPENAI_API_KEY` | OpenAI API key | `export OPENAI_API_KEY="sk-..."` |
| `REMEM_DATA_DIR` | Custom root data directory (defaults to `~/.remem`) | `export REMEM_DATA_DIR="/var/data/remem"` |

## Quickstart

### 1. Auto-Configure AI Assistants (`remem init`)

Automatically inject `remem` memory into your AI coding assistant with a single command:

```bash
# Configure Cursor
remem init cursor --project my-project

# Configure Claude Code
remem init claude-code --project my-project

# Configure all supported agents in your workspace
remem init all --project my-project
```

### 2. Model Context Protocol (MCP) Manual Setup

Add `remem` to your tool's MCP configuration (`.cursor/mcp.json`, `claude_code_config.json`, etc.):

```json
{
  "mcpServers": {
    "remem": {
      "command": "remem",
      "args": ["mcp", "--project", "my-project"]
    }
  }
}
```

### 3. Python SDK Usage

```python
import asyncio
from rememhq import Memory

async def main():
    # Uses provider from environment (REMEM_PROVIDER=gemini, GOOGLE_API_KEY=...)
    m = Memory(project="my-agent")

    # Store durable preferences or facts
    await m.store("The production database uses PostgreSQL 15 on RDS with SSL", tags=["infra", "db"])

    # Recall with guided LLM reasoning & RRF hybrid search
    results = await m.recall("what database are we using?")
    for r in results:
        print(f"Content: {r.content}")
        print(f"Reasoning: {r.reasoning}")

asyncio.run(main())
```

### 4. TypeScript SDK Usage

```typescript
import { Memory } from "@rememhq/sdk";

const m = new Memory({ project: "my-agent" });

// Store memory
await m.store("This repository uses trunk-based development", { tags: ["workflow"] });

// Recall with reasoning
const results = await m.recall("how do we manage branches?");
console.log(results);
```

## Usage Commands

### Local Development (Building from Source)

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

### Running the pre-built binaries (Downloaded Releases)

If you downloaded the pre-built binary releases from GitHub, you can use the `remem` executable directly.
Here are some of the basic commands:

```bash
# Start the REST API server
remem serve --project my-project

# Start the MCP server (stdio transport)
remem mcp --project my-project

# Store a memory
remem store "The main branch is called 'main'"

# Recall memories with guided retrieval
remem recall "What is the main branch called?"

# Search memories (no LLM re-ranking)
remem search "main branch"

# Show database statistics
remem inspect

# Apply importance-weighted decay to all active memories
remem decay

# Start an interactive REPL mode
remem repl

# Start the Remem AI terminal agent (uses native tool calling to run shell commands)
# Ensure REMEM_PROVIDER is set to anthropic, openai, gemini, or local
remem agent

# List downloaded local models
remem models list

# Pull a local model for offline use
remem models pull nomic-embed-text

# Bulk import memories from a JSONL file
remem import data.jsonl

# Export all memories to a JSONL file
remem export backup.jsonl

# Initialize MCP configurations for all supported agents in your workspace
remem init all --project my-project

# Initialize MCP config for a specific agent (e.g. claude-code, windsurf, roocode)
remem init claude-code --project my-project
```

### Running Services from Source

If you are developing locally, you can run the components via `cargo`:

```bash
# Start the API server (REST interface)
cargo run -p rememhq-api -- --project default

# Start the MCP server (stdio interface for Claude Code, Cursor, etc.)
cargo run -p rememhq-mcp

# Run the CLI tool
cargo run -p rememhq-cli -- --help

# Run the Remen AI terminal agent
cargo run -p rememhq-cli -- agent
```

### SDKs

```bash
# Python SDK
cd sdk/python && pip install -e ".[dev]" && pytest tests/

# TypeScript SDK
cd sdk/typescript && npm install && npm run build
```

## Contributing

We welcome contributions! Whether you're fixing a bug, improving the reasoning prompts, or adding a new provider, please check out our [CONTRIBUTING.md](CONTRIBUTING.md).

1.  Clone the repo: `git clone https://github.com/remem-io/remem`
2.  Build: `cargo build`
3.  Test: `cargo test --workspace`

## License

remem is licensed under the **Apache License 2.0**. See [LICENSE](LICENSE) for details.
