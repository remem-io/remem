//! remem CLI — manage, serve, and inspect AI agent memory.
//!
//! Commands:
//! - remem serve          — start the REST API server
//! - remem mcp            — start the MCP server (stdio)
//! - remem store `<text>`   — store a memory
//! - remem recall `<query>` — recall memories
//! - remem inspect        — show database statistics

use clap::{Parser, Subcommand};
use std::sync::Arc;

use remem_core::config::RememConfig;
use remem_core::memory::types::{MemoryRecord, MemoryType};
use remem_core::providers::anthropic::AnthropicProvider;
use remem_core::providers::embeddings::OpenAIEmbeddings;
use remem_core::providers::google::{GoogleEmbeddings, GoogleProvider};
use remem_core::providers::openai::OpenAIProvider;
use remem_core::reasoning::ReasoningEngine;
use remem_core::storage::sqlite::SqliteStore;
use remem_core::storage::vector::{HNSWVectorIndex, VectorIndex};
use remem_core::storage::MemoryStore;

#[derive(Parser)]
#[command(
    name = "remem",
    version = "0.1.0",
    about = "Reasoning memory layer for AI agents"
)]
struct Cli {
    /// Project name for memory isolation
    #[arg(long, global = true, default_value = "default")]
    project: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the REST API server
    Serve {
        #[arg(long, default_value = "7474")]
        port: u16,
    },
    /// Start the MCP server (stdio transport)
    Mcp,
    /// Store a memory
    Store {
        /// Content to store
        content: String,
        /// Tags (comma-separated)
        #[arg(long)]
        tags: Option<String>,
        /// Importance score (1-10)
        #[arg(long)]
        importance: Option<f32>,
        /// Memory type
        #[arg(long, default_value = "fact")]
        r#type: String,
    },
    /// Recall memories with guided retrieval
    Recall {
        /// Query string
        query: String,
        /// Max results
        #[arg(long, default_value = "8")]
        limit: usize,
    },
    /// Search memories (no LLM re-ranking)
    Search {
        /// Query string
        query: String,
        /// Max results
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Show database statistics
    Inspect,
    /// Apply importance-weighted decay to all active memories
    Decay {
        /// Decay factor (0.0 to 1.0, lower means faster decay)
        #[arg(long, default_value = "0.9")]
        factor: f32,
    },
    /// Model management
    Models {
        #[command(subcommand)]
        action: ModelAction,
    },
}

#[derive(Subcommand)]
enum ModelAction {
    /// Pull a model
    Pull {
        /// Model name (e.g., "nomic-embed", "phi-3-mini")
        name: String,
    },
    /// List downloaded models
    List,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter("remem=info")
        .init();

    let cli = Cli::parse();
    let config = RememConfig::load(&cli.project, None)?;

    match cli.command {
        Commands::Serve { port } => {
            println!("remem REST API starting on port {}...", port);
            println!("Project: {}", cli.project);
            println!("Provider: {}", config.reasoning.provider);
            println!("Data dir: {}", config.project_data_dir().display());

            // Delegate to remem-api binary
            let status = std::process::Command::new("remem-api")
                .args(["--port", &port.to_string(), "--project", &cli.project])
                .status();

            match status {
                Ok(s) if s.success() => Ok(()),
                Ok(s) => anyhow::bail!("remem-api exited with status: {}", s),
                Err(_) => {
                    println!("remem-api binary not found. Run: cargo install --path remem-api");
                    anyhow::bail!("remem-api not found")
                }
            }
        }

        Commands::Mcp => {
            println!("remem MCP server starting (stdio)...");
            let status = std::process::Command::new("remem-mcp")
                .args(["--project", &cli.project])
                .status();

            match status {
                Ok(s) if s.success() => Ok(()),
                Ok(s) => anyhow::bail!("remem-mcp exited with status: {}", s),
                Err(_) => {
                    println!("remem-mcp binary not found. Run: cargo install --path remem-mcp");
                    anyhow::bail!("remem-mcp not found")
                }
            }
        }

        Commands::Store {
            content,
            tags,
            importance,
            r#type,
        } => {
            let engine = build_engine(&config).await?;

            let memory_type: MemoryType = r#type.parse().unwrap_or(MemoryType::Fact);
            let tag_list: Vec<String> = tags
                .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default();

            let auto_score = importance.is_none();
            let mut record = MemoryRecord::new(&content, memory_type).with_tags(tag_list);
            if let Some(imp) = importance {
                record = record.with_importance(imp);
            }

            let stored = engine.store_memory(record, auto_score).await?;
            println!("✓ Stored memory {}", stored.id);
            println!("  importance: {:.1}", stored.importance);
            println!("  tags: {:?}", stored.tags);
            println!("  type: {}", stored.memory_type);

            // Save index
            engine.index.save(&config.index_path()).await?;
            Ok(())
        }

        Commands::Recall { query, limit } => {
            let engine = build_engine(&config).await?;
            let results = engine.recall(&query, limit, &[], None, None).await?;

            if results.is_empty() {
                println!("No memories found for: \"{}\"", query);
            } else {
                println!("Found {} memories:\n", results.len());
                for (i, r) in results.iter().enumerate() {
                    println!(
                        "  {}. [imp: {:.1}, decay: {:.2}] {}",
                        i + 1,
                        r.importance,
                        r.decay_score,
                        r.content
                    );
                    if let Some(reasoning) = &r.reasoning {
                        println!("     → {}", reasoning);
                    }
                    println!();
                }
            }
            Ok(())
        }

        Commands::Search { query, limit } => {
            let engine = build_engine(&config).await?;
            let results = engine.search(&query, limit, &[]).await?;

            if results.is_empty() {
                println!("No memories found for: \"{}\"", query);
            } else {
                println!("Found {} memories:\n", results.len());
                for (i, r) in results.iter().enumerate() {
                    println!(
                        "  {}. [sim: {:.3}, imp: {:.1}, decay: {:.2}] {}",
                        i + 1,
                        r.similarity,
                        r.importance,
                        r.decay_score,
                        r.content
                    );
                }
            }
            Ok(())
        }

        Commands::Inspect => {
            let store = SqliteStore::open(&config.db_path())?;
            let stats = store.stats().await?;

            println!("remem database: {}", config.db_path().display());
            println!("  Total memories: {}", stats.total_memories);
            println!("  Average importance: {:.1}", stats.avg_importance);
            println!("  By type:");
            for (k, v) in &stats.by_type {
                println!("    {}: {}", k, v);
            }
            Ok(())
        }
        Commands::Decay { factor } => {
            let engine = build_engine(&config).await?;
            let archived_count = engine.apply_decay(factor).await?;
            println!("✓ Applied decay with factor {}", factor);
            println!("  Archived {} memories", archived_count);

            // Save index since we removed archived items
            engine.index.save(&config.index_path()).await?;
            Ok(())
        }

        Commands::Models { action } => match action {
            ModelAction::Pull { name } => {
                println!("Model pulling not yet implemented in v0.1 (using cloud APIs).");
                println!("Requested: {}", name);
                println!("Local models will be available in v0.2.");
                Ok(())
            }
            ModelAction::List => {
                println!("v0.1 uses cloud APIs — no local models required.");
                println!("Local model support coming in v0.2.");
                Ok(())
            }
        },
    }
}

/// Build a reasoning engine from config (shared setup for CLI commands).
async fn build_engine(config: &RememConfig) -> anyhow::Result<ReasoningEngine> {
    let store = Arc::new(SqliteStore::open(&config.db_path())?);
    let index = Arc::new(HNSWVectorIndex::new(768, 10000));
    let _ = index.load(&config.index_path()).await;

    let provider: Arc<dyn remem_core::providers::Provider> =
        match config.reasoning.provider.as_str() {
            "openai" => Arc::new(OpenAIProvider::new(None)?),
            "google" => Arc::new(GoogleProvider::new(None)?),
            "mock" => Arc::new(remem_core::providers::mock::MockProvider),
            _ => match AnthropicProvider::new(None) {
                Ok(p) => Arc::new(p),
                Err(_) => Arc::new(OpenAIProvider::new(None)?),
            },
        };

    let embeddings: Arc<dyn remem_core::providers::EmbeddingProvider> =
        match config.reasoning.provider.as_str() {
            "google" => Arc::new(GoogleEmbeddings::new(None)?),
            "mock" => Arc::new(remem_core::providers::mock::MockEmbeddings::new(768)),
            _ => Arc::new(OpenAIEmbeddings::new(None, Some(768))?),
        };

    Ok(ReasoningEngine::new(
        config.clone(),
        provider,
        embeddings,
        store,
        index,
    ))
}
