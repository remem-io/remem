# rememhq-api

[![Crates.io](https://img.shields.io/crates/v/rememhq-api.svg)](https://crates.io/crates/rememhq-api)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

The REST API server for **remem** — the reasoning memory layer for AI agents.

This crate provides a highly concurrent, fully featured REST HTTP server powered by `axum` and `tokio`. It exposes the `rememhq-core` functionalities (memory storage, semantic recall, reasoning, and search) over a network API, serving as the backend for the official TypeScript and Python SDKs.

## Installation

You can install the API server directly via `cargo`:

```bash
cargo install rememhq-api
```

## Usage

Start the server, specifying a default namespace (project) to isolate memory contexts:

```bash
rememhq-api --project my-agent
```

By default, the server listens on `http://127.0.0.1:7474`. You can configure the port and host via environment variables or CLI flags.

### Supported Operations

The API provides endpoints for all memory capabilities:
- `POST /v1/memories` - Store a new memory.
- `GET /v1/memories/recall` - Trigger LLM-guided recall.
- `GET /v1/memories/search` - Perform vector similarity search.
- `PUT /v1/memories/:id` - Update existing memory facts.
- `POST /v1/session/consolidate` - Condense scratchpad sessions into durable memory.

## Ecosystem

- **TypeScript SDK**: `@rememhq/sdk`
- **Python SDK**: `rememhq`
- **Rust SDK**: `rememhq` (direct embedding, no API required)

## License

Apache License 2.0. See the [LICENSE](../LICENSE) file.
