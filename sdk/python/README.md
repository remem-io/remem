# remem Python SDK

[![PyPI version](https://badge.fury.io/py/rememhq.svg)](https://badge.fury.io/py/rememhq)
[![Python 3.11+](https://img.shields.io/badge/python-3.11+-blue.svg)](https://www.python.org/downloads/)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

The official Python SDK for **remem** — the reasoning memory layer for AI agents.

Remem provides a persistent, queryable memory system that uses LLM-powered reasoning for importance scoring, contradiction detection, knowledge graph construction, and session consolidation. This SDK offers a strongly-typed, fully asynchronous interface over the remem REST API using `httpx` and `pydantic`.

## Key Features

- **Fully Asynchronous**: Built natively on `asyncio` and `httpx`.
- **Type-Safe Models**: Robust validation and autocompletion powered by Pydantic.
- **LLM-Powered Reasoning**: Effortlessly invoke semantic recall, importance scoring, and consolidation.
- **Knowledge Graph**: Native support for storing and querying relationship triples.

## Installation

Install the package via pip or your preferred package manager (e.g., uv, poetry):

```bash
pip install rememhq
```

## Quick Start

### 1. Start the remem API Server

Before using the SDK, start the `rememhq-api` server from the remem repository root:

```bash
cargo run -p rememhq-api -- --project default
```

By default, the server listens on `http://localhost:7474`.

### 2. Initialize the Client

```python
import asyncio
from rememhq import Memory

async def main():
    # Initialize the memory client
    memory = Memory(
        base_url="http://localhost:7474",
        project="my-agent",
        reasoning_model="claude-sonnet-4-6"  # Configure the underlying reasoning engine
    )

    # Store a memory
    store_resp = await memory.store(
        content="The user prefers Python for ML tasks.",
        tags=["preferences", "ml"]
    )
    print(f"Stored memory ID: {store_resp.id}")

    # Recall memories using semantic reasoning
    results = await memory.recall(query="What language does the user prefer for machine learning?", limit=5)
    
    for result in results:
        print(f"Content: {result.content}")
        print(f"Reasoning trace: {result.reasoning}")

if __name__ == "__main__":
    asyncio.run(main())
```

## Configuration Options

You can configure the SDK using constructor parameters or environment variables:

```python
memory = Memory(
    base_url="http://localhost:7474",
    project="my-agent",
    reasoning_model="gpt-4o",
    timeout=30.0
)
```

**Environment Variables:**
- `REMEM_API_URL` — API server URL (default: `http://localhost:7474`)
- `REMEM_PROJECT` — Target project namespace (default: `default`)
- `REMEM_REASONING_MODEL` — Reasoning model (default: `claude-sonnet-4-6`)
- `REMEM_TIMEOUT` — Request timeout in seconds (default: `30`)

## API Reference

### `Memory` Methods

- `store(content: str, tags: list[str] = None, importance: float = None) -> StoreResponse`: Store a new memory.
- `recall(query: str, limit: int = 8) -> list[RecallResult]`: Retrieve contextually relevant memories utilizing LLM evaluation.
- `search(query: str, limit: int = 10) -> list[SearchResult]`: Execute rapid full-text/vector search.
- `update(memory_id: str, content: str) -> UpdateResponse`: Modify existing memory content.
- `forget(memory_id: str) -> ForgetResponse`: Delete a specific memory item.
- `consolidate(session_id: str) -> ConsolidateResponse`: Transform short-term session logs into long-term durable facts.

## Development

To develop the SDK locally:

```bash
cd sdk/python

# Install dependencies (requires Python 3.11+)
pip install -e ".[dev]"

# Run tests
pytest tests/ -v

# Format and Lint
black rememhq/ tests/
ruff check rememhq/ tests/
```

## Contributing

We welcome contributions! Please review our [Contributing Guide](../../CONTRIBUTING.md) for details on submitting pull requests, reporting issues, and suggesting enhancements.

## License

This project is licensed under the Apache License 2.0. See the [LICENSE](../../LICENSE) file for more details.
