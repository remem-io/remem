# remem Python SDK

This is the Python SDK for remem, providing a typed interface over the REST API.

## Installation

```bash
pip install rememhq
```

## Quick Start

```python
from rememhq import Memory

memory = Memory(base_url="http://localhost:7474")

# Store a memory
await memory.store("User prefers Python", tags=["language"])

# Recall memories
results = await memory.recall("what language does the user prefer?")
for result in results:
    print(f"Content: {result.content}")
    print(f"Reasoning: {result.reasoning}")
```

## Development

### Prerequisites

- **Python** 3.11+
- **pip** or **uv**

### Setup

```bash
cd sdk/python

# Install in development mode with dev dependencies
pip install -e ".[dev]"
```

### Testing

```bash
# Run all tests
pytest tests/ -v

# Run with coverage
pytest tests/ --cov=rememhq

# Run specific test
pytest tests/test_memory.py -v
```

### Code Quality

```bash
# Format code
black rememhq/ tests/

# Lint
ruff check rememhq/ tests/

# Type checking
mypy rememhq/
```

### Building

```bash
# Build distribution
python -m build

# Install locally from build
pip install dist/rememhq-*.whl
```

## Running the API Server

Before using the SDK, start the remem API server from the repository root:

```bash
cargo run -p rememhq-api -- --project default
```

The API will be available at `http://localhost:7474` by default.

## API Reference

### Memory

#### `store(content: str, tags: Optional[List[str]] = None, importance: Optional[float] = None) -> StoreResponse`

Store a memory with optional tags and importance score.

```python
response = await memory.store(
    "Production database is PostgreSQL 15",
    tags=["infra", "database"],
    importance=0.95
)
```

#### `recall(query: str, limit: int = 8) -> List[RecallResult]`

Retrieve memories most relevant to the query, with reasoning traces.

```python
results = await memory.recall("what database are we using?", limit=8)
for result in results:
    print(f"Content: {result.content}")
    print(f"Score: {result.score}")
    print(f"Reasoning: {result.reasoning}")
```

#### `search(query: str, limit: int = 10) -> List[SearchResult]`

Full-text search over all memories.

```python
results = await memory.search("deployment", limit=10)
```

#### `update(memory_id: str, content: str) -> UpdateResponse`

Update an existing memory's content.

#### `forget(memory_id: str) -> ForgetResponse`

Delete a memory.

#### `consolidate(session_id: str) -> ConsolidateResponse`

Consolidate session logs into durable facts.

## Configuration

Configure the SDK via constructor parameters or environment variables:

```python
from rememhq import Memory

memory = Memory(
    base_url="http://localhost:7474",
    project="my-agent",
    reasoning_model="claude-sonnet-4-6",
    timeout=30.0
)
```

**Environment variables:**
- `REMEM_API_URL` — API server URL (default: `http://localhost:7474`)
- `REMEM_PROJECT` — Project name (default: `default`)
- `REMEM_REASONING_MODEL` — Reasoning model (default: `claude-sonnet-4-6`)
- `REMEM_TIMEOUT` — Request timeout in seconds (default: `30`)

## Examples

Check the `examples/` directory for complete working scripts.

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for guidelines on contributing to the Python SDK.

## License

Apache License 2.0 — See [LICENSE](../../LICENSE) for details.
