# remem Rust SDK

Thin, high-level async wrapper around `rememhq-core` for direct Rust integration.

## Installation

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
    let mem = Memory::builder()
        .project("my-agent")
        .reasoning_model(ReasoningModel::ClaudeSonnet)
        .build()
        .await?;

    // Store a memory (auto-scored by LLM)
    let record = mem.store(
        "rate limiting uses a token bucket at 1000 req/min",
        &["api", "limits"],
        None,
    ).await?;
    println!("Stored: {} (importance: {:.1})", record.id, record.importance);

    // Store a typed memory
    let _proc = mem.store_typed(
        "To deploy: run `make deploy` then verify on /health",
        MemoryType::Procedure,
        &["devops"],
        Some(9.0),
    ).await?;

    // LLM-guided recall (semantic search + re-ranking)
    let results = mem.recall("api rate limits", 5).await?;
    for r in &results {
        println!("{} (importance: {:.1})", r.content, r.importance);
    }

    // Simple vector search (no LLM)
    let search_results = mem.search("deploy", 10).await?;
    
    // Knowledge graph query
    let triples = mem.query_knowledge(Some("Bob"), None, None).await?;

    // Forget a memory
    mem.forget(record.id, ForgetMode::Delete).await?;

    // Save index before exiting
    mem.save_index().await?;

    Ok(())
}
```

## Model Presets

| Preset | Provider | Model ID |
|---|---|---|
| `ClaudeSonnet` | Anthropic | `claude-sonnet-4-5` |
| `ClaudeHaiku` | Anthropic | `claude-haiku-4-5` |
| `Gpt4o` | OpenAI | `gpt-4o` |
| `Gpt4oMini` | OpenAI | `gpt-4o-mini` |
| `Gemini2Flash` | Google | `gemini-2.0-flash` |
| `Custom(name)` | Anthropic | User-provided |

## Provider Auto-Detection

If no API key is available for the configured provider, the SDK
automatically falls back through the chain:

1. **Configured provider** (from env or builder)
2. **Anthropic** → **OpenAI** → **Google** (whichever key is set)
3. **MockProvider** (returns placeholder responses for offline development)

Set your keys in the environment:
```bash
export ANTHROPIC_API_KEY=sk-ant-...
export OPENAI_API_KEY=sk-...
export GOOGLE_API_KEY=...
```

## Advanced Usage

```rust
// Access the underlying ReasoningEngine directly
let engine = mem.engine();
let stats = engine.store.stats().await?;
println!("Total memories: {}", stats.total_memories);
```

## Status

✅ **v0.1.0** — Full builder API with auto-detection, cascading fallback, and all memory operations.
