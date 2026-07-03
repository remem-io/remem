# rememhq-core

[![Crates.io](https://img.shields.io/crates/v/rememhq-core.svg)](https://crates.io/crates/rememhq-core)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

The core orchestration library for **remem** — the reasoning memory layer for AI agents.

This crate encapsulates the foundational business logic, persistent SQLite storage, high-speed C++ FFI vector indexing (HNSW), and provider abstraction layers (Anthropic, OpenAI, Google) that power the remem system. 

It is designed to be the highly optimized, memory-safe backbone of remem. Other components, such as `rememhq-api`, `rememhq-mcp`, and the `rememhq` rust SDK, are all built upon this core.

## Features

- **Robust Storage**: Uses SQLite for transactional, structured memory persistence.
- **Fast Vector Index**: Interops with C++ via FFI (`libremem-sys`) for blazing-fast HNSW nearest-neighbor vector search.
- **Reasoning Engine**: Implements the advanced logic to handle semantic recall, memory evaluation, and conflict resolution using foundation models.
- **Provider Agnostic**: Unified traits for text embeddings and chat completions across leading AI providers.

## Integration

Unless you are building customized deployments or low-level integrations, we highly recommend using the higher-level [`rememhq`](https://crates.io/crates/rememhq) crate for your rust applications.

If you wish to use the core library directly:

```toml
[dependencies]
rememhq-core = "0.1"
```

## Contributing

Please refer to the [Contributing Guide](../CONTRIBUTING.md) in the root repository for information on how to test and contribute to the core library.

## License

Apache License 2.0. See the [LICENSE](../LICENSE) file.
