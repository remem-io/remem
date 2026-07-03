# rememhq

[![Crates.io](https://img.shields.io/crates/v/rememhq.svg)](https://crates.io/crates/rememhq)
[![Documentation](https://docs.rs/rememhq/badge.svg)](https://docs.rs/rememhq)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

The official high-level Rust SDK for **remem** — the reasoning memory layer for AI agents.

Remem provides a persistent, queryable memory system that uses LLM-powered reasoning for importance scoring, contradiction detection, knowledge graph construction, and session consolidation. This SDK is a thin, ergonomic, asynchronous wrapper around the `rememhq-core` engine, allowing direct embedding into Rust applications.

## Key Features

- **Direct Embedding**: No HTTP overhead. Runs the core SQLite/Vector Index engine directly in your process.
- **Provider Auto-Detection**: Seamlessly falls back between Anthropic, OpenAI, and Google depending on available API keys.
- **LLM Reasoning**: Advanced semantic recall, context condensation, and automated importance scoring natively orchestrated in Rust.
- **High Performance**: Leverages robust async I/O via `tokio` and C++ FFI for rapid vector searches (HNSW).

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
rememhq = "0.1"
tokio = { version = "1", features = ["full"] }
```

## Quick Start

```rust
use rememhq::{Memory, ReasoningModel, MemoryType, ForgetMode};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Initialize the memory engine
    let mem = Memory::builder()
        .project("my-agent")
        .reasoning_model(ReasoningModel::ClaudeSonnet)
        .build()
        .await?;

    // 2. Store a memory (auto-scored for importance by the LLM)
    let record = mem.store(
        "User prefers using Rust for performance-critical systems",
        &["preferences", "languages"],
        None,
    ).await?;
    
    println!("Stored memory ID: {} (importance: {:.1})", record.id, record.importance);

    // 3. LLM-guided recall (Semantic Search + LLM Re-ranking)
    let results = mem.recall("What is the user's preferred systems language?", 5).await?;
    for r in &results {
        println!("Content: {} (Score: {:.1})", r.content, r.importance);
    }

    // 4. Save the underlying vector index to disk before exiting
    mem.save_index().await?;

    Ok(())
}
```

## Model Presets & Providers

The SDK supports several top-tier foundation models out of the box:

| Preset | Provider | Default Model ID |
|---|---|---|
| `ClaudeSonnet` | Anthropic | `claude-sonnet-4-5` |
| `ClaudeHaiku` | Anthropic | `claude-haiku-4-5` |
| `Gpt4o` | OpenAI | `gpt-4o` |
| `Gpt4oMini` | OpenAI | `gpt-4o-mini` |
| `Gemini2Flash` | Google | `gemini-2.0-flash` |
| `Custom(name)` | Auto | User-provided |

### Auto-Detection

If no API key is directly provided, the SDK will automatically fallback through the available environment variables in this order:
1. `ANTHROPIC_API_KEY`
2. `OPENAI_API_KEY`
3. `GOOGLE_API_KEY`

It also features a `MockProvider` for offline development and test environments when no keys are detected.

## Advanced Usage

For deep customizations, you can access the underlying `ReasoningEngine` and `MemoryStore` components directly:

```rust
// Access the internal reasoning engine
let engine = mem.engine();

// Directly query SQLite statistics
let stats = engine.store.stats().await?;
println!("Total memories tracking: {}", stats.total_memories);
```

## Contributing

We welcome contributions! Please refer to the [Contributing Guide](../../CONTRIBUTING.md) in the repository root for guidelines on development, testing, and PR submission.

## License

This project is licensed under the Apache License 2.0. See the [LICENSE](../../LICENSE) file for more details.
