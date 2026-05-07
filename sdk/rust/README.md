# Rust SDK

Thin async wrapper around `rememhq-core` for direct Rust integration.

## Usage

```toml
[dependencies]
rememhq = "0.1"
tokio = { version = "1", features = ["full"] }
```

```rust
use rememhq::{Memory, ReasoningModel};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let m = Memory::builder()
        .project("my-agent")
        .reasoning_model(ReasoningModel::ClaudeSonnet)
        .build()
        .await?;

    m.store("rate limiting uses a token bucket at 1000 req/min", &["api", "limits"], None).await?;

    let results = m.recall("api rate limits", 5).await?;
    for r in &results {
        println!("{} (importance: {})", r.content, r.importance);
    }

    Ok(())
}
```

## Status
🚧 The Rust SDK is a re-export of `rememhq-core` types with a builder API.
Full implementation coming alongside the stable v1.0 API.
