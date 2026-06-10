//! remem CLI — manage, serve, and inspect AI agent memory.
//!
//! Commands:
//! - remem serve          — start the REST API server
//! - remem mcp            — start the MCP server (stdio)
//! - remem store `<text>`   — store a memory
//! - remem recall `<query>` — recall memories
//! - remem inspect        — show database statistics

use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::io::Write;
use std::sync::Arc;

use rememhq_core::config::RememConfig;
use rememhq_core::memory::types::{MemoryRecord, MemoryType};
use rememhq_core::providers::anthropic::AnthropicProvider;
use rememhq_core::providers::embeddings::OpenAIEmbeddings;
use rememhq_core::providers::google::{GoogleEmbeddings, GoogleProvider};
use rememhq_core::providers::openai::OpenAIProvider;
use rememhq_core::reasoning::ReasoningEngine;
use rememhq_core::storage::sqlite::SqliteStore;
use rememhq_core::storage::vector::{HNSWVectorIndex, VectorIndex};
use rememhq_core::storage::MemoryStore;

#[derive(Parser)]
#[command(
    name = "remem",
    version = env!("CARGO_PKG_VERSION"),
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
    /// Interactive REPL mode
    Repl,
    /// Bulk import memories from a JSONL file
    Import {
        /// Path to JSONL file (one JSON object per line)
        file: String,
    },
    /// Export all memories to a JSONL file
    Export {
        /// Output file path (defaults to stdout)
        #[arg(long, short)]
        output: Option<String>,
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

            // Delegate to rememhq-api binary
            let status = std::process::Command::new("rememhq-api")
                .args(["--port", &port.to_string(), "--project", &cli.project])
                .status();

            match status {
                Ok(s) if s.success() => Ok(()),
                Ok(s) => anyhow::bail!("rememhq-api exited with status: {}", s),
                Err(_) => {
                    println!("rememhq-api binary not found. Run: cargo install --path rememhq-api");
                    anyhow::bail!("rememhq-api not found")
                }
            }
        }

        Commands::Mcp => {
            println!("remem MCP server starting (stdio)...");
            let status = std::process::Command::new("rememhq-mcp")
                .args(["--project", &cli.project])
                .status();

            match status {
                Ok(s) if s.success() => Ok(()),
                Ok(s) => anyhow::bail!("rememhq-mcp exited with status: {}", s),
                Err(_) => {
                    println!("rememhq-mcp binary not found. Run: cargo install --path rememhq-mcp");
                    anyhow::bail!("rememhq-mcp not found")
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
                let spec = rememhq_core::models::find_model(&name).ok_or_else(|| {
                    let known: Vec<&str> = rememhq_core::models::KNOWN_MODELS
                        .iter()
                        .map(|m| m.id)
                        .collect();
                    anyhow::anyhow!(
                        "Unknown model '{}'. Available models: {}",
                        name,
                        known.join(", ")
                    )
                })?;

                let dest = rememhq_core::models::default_models_dir();
                println!("Pulling '{}' → {}", spec.id, dest.display());
                println!("  {}", spec.description);
                println!("  (approx. {:.0} MB)", spec.approx_bytes as f64 / 1_000_000.0);

                let result = rememhq_core::models::pull_model(spec, &dest).await?;

                if result.onnx_downloaded {
                    println!("  ✓ Downloaded {}", spec.onnx_filename);
                } else {
                    println!("  ✓ {} already present (skipped)", spec.onnx_filename);
                }
                if result.vocab_downloaded {
                    println!("  ✓ Downloaded {}", spec.vocab_filename);
                } else {
                    println!("  ✓ {} already present (skipped)", spec.vocab_filename);
                }

                println!("\nModel ready. Set environment variables to use it:");
                println!(
                    "  REMEM_PROVIDER=local \\\n  REMEM_LOCAL_MODEL_PATH={} \\\n  REMEM_LOCAL_VOCAB_PATH={}",
                    result.onnx_path.display(),
                    result.vocab_path.display()
                );

                Ok(())
            }

            ModelAction::List => {
                let dest = rememhq_core::models::default_models_dir();
                println!("Known models (model dir: {}):\n", dest.display());

                for spec in rememhq_core::models::KNOWN_MODELS {
                    let onnx_present = dest.join(spec.onnx_filename).exists();
                    let vocab_present = dest.join(spec.vocab_filename).exists();
                    let status = match (onnx_present, vocab_present) {
                        (true, true) => "✓ installed",
                        (true, false) => "⚠ onnx present, vocab missing",
                        (false, true) => "⚠ vocab present, onnx missing",
                        (false, false) => "  not installed",
                    };
                    println!("  {:14} {}  —  {}", spec.id, status, spec.description);
                }

                println!("\nTo install a model run:  remem models pull <id>");
                Ok(())
            }
        },

        Commands::Repl => {
            let engine = build_engine(&config).await?;
            run_repl(engine, &config).await
        }

        Commands::Import { file } => {
            let engine = build_engine(&config).await?;
            let path = std::path::Path::new(&file);
            if !path.exists() {
                anyhow::bail!("File not found: {}", file);
            }

            let content = std::fs::read_to_string(path)?;
            let mut imported = 0;
            let mut errors = 0;

            for (i, line) in content.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                match serde_json::from_str::<ImportRecord>(line) {
                    Ok(rec) => {
                        let memory_type: MemoryType = rec
                            .memory_type
                            .as_deref()
                            .unwrap_or("fact")
                            .parse()
                            .unwrap_or(MemoryType::Fact);
                        let auto_score = rec.importance.is_none();
                        let mut record = MemoryRecord::new(&rec.content, memory_type)
                            .with_tags(rec.tags.unwrap_or_default());
                        if let Some(imp) = rec.importance {
                            record = record.with_importance(imp);
                        }
                        match engine.store_memory(record, auto_score).await {
                            Ok(stored) => {
                                imported += 1;
                                println!(
                                    "  ✓ [{}] {} (id: {})",
                                    imported,
                                    stored.content.chars().take(60).collect::<String>(),
                                    stored.id
                                );
                            }
                            Err(e) => {
                                errors += 1;
                                eprintln!("  ✗ Line {}: {}", i + 1, e);
                            }
                        }
                    }
                    Err(e) => {
                        errors += 1;
                        eprintln!("  ✗ Line {}: Parse error: {}", i + 1, e);
                    }
                }
            }

            println!(
                "\n✓ Import complete: {} imported, {} errors",
                imported, errors
            );

            // Save index
            engine.index.save(&config.index_path()).await?;
            Ok(())
        }

        Commands::Export { output } => {
            let store = SqliteStore::open(&config.db_path())?;
            let all_records = store.list(&[], None, None, 100_000).await?;

            let mut writer: Box<dyn std::io::Write> = match &output {
                Some(path) => Box::new(std::fs::File::create(path)?),
                None => Box::new(std::io::stdout()),
            };

            let mut count = 0;
            for record in &all_records {
                let export = ExportRecord {
                    id: record.id.to_string(),
                    content: record.content.clone(),
                    memory_type: record.memory_type.to_string(),
                    tags: record.tags.clone(),
                    importance: record.importance,
                    decay_score: record.decay_score,
                    created_at: record.created_at.to_rfc3339(),
                    updated_at: record.updated_at.to_rfc3339(),
                };
                let json = serde_json::to_string(&export)?;
                writeln!(writer, "{}", json)?;
                count += 1;
            }

            if let Some(path) = &output {
                println!("✓ Exported {} memories to {}", count, path);
            } else {
                eprintln!("✓ Exported {} memories to stdout", count);
            }
            Ok(())
        }
    }
}

/// Build a reasoning engine from config (shared setup for CLI commands).
///
/// Uses cascading fallback: configured provider → alternatives → MockProvider.
async fn build_engine(config: &RememConfig) -> anyhow::Result<ReasoningEngine> {
    let store = Arc::new(SqliteStore::open(&config.db_path())?);
    let index = Arc::new(HNSWVectorIndex::new(768, 10000));
    let _ = index.load(&config.index_path()).await;

    // Reasoning provider with robust fallback
    let provider: Arc<dyn rememhq_core::providers::Provider> =
        match config.reasoning.provider.as_str() {
            "openai" => match OpenAIProvider::new(None) {
                Ok(p) => Arc::new(p),
                Err(e) => {
                    eprintln!(
                        "⚠ Failed to initialize OpenAI provider: {}. Trying fallbacks...",
                        e
                    );
                    match AnthropicProvider::new(None) {
                        Ok(p) => Arc::new(p),
                        Err(_) => match GoogleProvider::new(None) {
                            Ok(p) => Arc::new(p),
                            Err(_) => {
                                eprintln!("⚠ No valid API keys found. Using MockProvider.");
                                Arc::new(rememhq_core::providers::mock::MockProvider)
                            }
                        },
                    }
                }
            },
            "anthropic" => match AnthropicProvider::new(None) {
                Ok(p) => Arc::new(p),
                Err(e) => {
                    eprintln!(
                        "⚠ Failed to initialize Anthropic provider: {}. Trying fallbacks...",
                        e
                    );
                    match OpenAIProvider::new(None) {
                        Ok(p) => Arc::new(p),
                        Err(_) => match GoogleProvider::new(None) {
                            Ok(p) => Arc::new(p),
                            Err(_) => {
                                eprintln!("⚠ No valid API keys found. Using MockProvider.");
                                Arc::new(rememhq_core::providers::mock::MockProvider)
                            }
                        },
                    }
                }
            },
            "google" => match GoogleProvider::new(None) {
                Ok(p) => Arc::new(p),
                Err(e) => {
                    eprintln!(
                        "⚠ Failed to initialize Google provider: {}. Trying fallbacks...",
                        e
                    );
                    match AnthropicProvider::new(None) {
                        Ok(p) => Arc::new(p),
                        Err(_) => match OpenAIProvider::new(None) {
                            Ok(p) => Arc::new(p),
                            Err(_) => {
                                eprintln!("⚠ No valid API keys found. Using MockProvider.");
                                Arc::new(rememhq_core::providers::mock::MockProvider)
                            }
                        },
                    }
                }
            },
            "local" => Arc::new(rememhq_core::providers::local::LocalProvider::new(None)),
            "mock" => Arc::new(rememhq_core::providers::mock::MockProvider),
            _ => {
                // Auto-detect based on env vars
                if std::env::var("ANTHROPIC_API_KEY").is_ok() {
                    match AnthropicProvider::new(None) {
                        Ok(p) => Arc::new(p),
                        Err(_) => Arc::new(rememhq_core::providers::mock::MockProvider),
                    }
                } else if std::env::var("OPENAI_API_KEY").is_ok() {
                    match OpenAIProvider::new(None) {
                        Ok(p) => Arc::new(p),
                        Err(_) => Arc::new(rememhq_core::providers::mock::MockProvider),
                    }
                } else if std::env::var("GOOGLE_API_KEY").is_ok() {
                    match GoogleProvider::new(None) {
                        Ok(p) => Arc::new(p),
                        Err(_) => Arc::new(rememhq_core::providers::mock::MockProvider),
                    }
                } else {
                    eprintln!("⚠ No reasoning API keys set. Using MockProvider.");
                    Arc::new(rememhq_core::providers::mock::MockProvider)
                }
            }
        };

    // Embedding provider with robust fallback
    let embeddings: Arc<dyn rememhq_core::providers::EmbeddingProvider> = match config
        .reasoning
        .provider
        .as_str()
    {
        "google" => match GoogleEmbeddings::new(None) {
            Ok(p) => Arc::new(p),
            Err(e) => {
                eprintln!(
                    "⚠ Failed to initialize Google embeddings: {}. Trying fallbacks...",
                    e
                );
                if std::env::var("OPENAI_API_KEY").is_ok() {
                    match OpenAIEmbeddings::new(None, Some(768)) {
                        Ok(p) => Arc::new(p),
                        Err(_) => Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(768)),
                    }
                } else {
                    Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(768))
                }
            }
        },
        "mock" => Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(768)),
        "local" => {
            let model_path = std::env::var("REMEM_LOCAL_MODEL_PATH")
                .unwrap_or_else(|_| "models/nomic-embed-text.onnx".to_string());
            let vocab_path = std::env::var("REMEM_LOCAL_VOCAB_PATH")
                .unwrap_or_else(|_| "models/vocab.txt".to_string());
            match rememhq_core::providers::local::LocalEmbeddings::new(&model_path, &vocab_path) {
                Ok(p) => Arc::new(p),
                Err(e) => {
                    eprintln!(
                        "⚠ Failed to initialize local embeddings: {}. Using MockEmbeddings.",
                        e
                    );
                    Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(768))
                }
            }
        }
        _ => {
            // Auto-detect embeddings
            if std::env::var("OPENAI_API_KEY").is_ok() {
                match OpenAIEmbeddings::new(None, Some(768)) {
                    Ok(p) => Arc::new(p),
                    Err(_) => Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(768)),
                }
            } else if std::env::var("GOOGLE_API_KEY").is_ok() {
                match GoogleEmbeddings::new(None) {
                    Ok(p) => Arc::new(p),
                    Err(_) => Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(768)),
                }
            } else {
                eprintln!("⚠ No embedding API keys found. Using MockEmbeddings.");
                Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(768))
            }
        }
    };

    Ok(ReasoningEngine::new(
        config.clone(),
        provider,
        embeddings,
        store,
        index,
    ))
}

// --- Import / Export record types ---

#[derive(Deserialize)]
struct ImportRecord {
    content: String,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    importance: Option<f32>,
    #[serde(default)]
    memory_type: Option<String>,
}

#[derive(serde::Serialize)]
struct ExportRecord {
    id: String,
    content: String,
    memory_type: String,
    tags: Vec<String>,
    importance: f32,
    decay_score: f32,
    created_at: String,
    updated_at: String,
}

// --- REPL ---

async fn run_repl(engine: ReasoningEngine, config: &RememConfig) -> anyhow::Result<()> {
    println!("remem interactive REPL v{}", env!("CARGO_PKG_VERSION"));
    println!("Project: {}", config.reasoning.provider);
    println!("Type 'help' for commands, 'quit' to exit.\n");

    let stdin = std::io::stdin();
    loop {
        print!("remem> ");
        std::io::stdout().flush()?;

        let mut input = String::new();
        if stdin.read_line(&mut input)? == 0 {
            // EOF
            break;
        }
        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let cmd = parts[0].to_lowercase();
        let args = parts.get(1).copied().unwrap_or("");

        match cmd.as_str() {
            "quit" | "exit" | "q" => {
                println!("Saving index and exiting...");
                engine.index.save(&config.index_path()).await?;
                break;
            }
            "help" | "h" | "?" => {
                println!("Commands:");
                println!("  store <text>         Store a memory (auto-scored)");
                println!("  recall <query>       Recall memories (LLM re-ranked)");
                println!("  search <query>       Search memories (vector + FTS)");
                println!("  inspect              Show database statistics");
                println!("  quit                 Save and exit");
                println!("  help                 Show this help");
            }
            "store" | "s" => {
                if args.is_empty() {
                    eprintln!("Usage: store <text>");
                    continue;
                }
                match engine
                    .store_memory(MemoryRecord::new(args, MemoryType::Fact), true)
                    .await
                {
                    Ok(stored) => {
                        println!(
                            "\u{2713} Stored {} (importance: {:.1})",
                            stored.id, stored.importance
                        );
                    }
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            "recall" | "r" => {
                if args.is_empty() {
                    eprintln!("Usage: recall <query>");
                    continue;
                }
                match engine.recall(args, 8, &[], None, None).await {
                    Ok(results) => {
                        if results.is_empty() {
                            println!("No memories found.");
                        } else {
                            for (i, r) in results.iter().enumerate() {
                                println!("  {}. [imp: {:.1}] {}", i + 1, r.importance, r.content);
                                if let Some(reasoning) = &r.reasoning {
                                    println!("     \u{2192} {}", reasoning);
                                }
                            }
                        }
                    }
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            "search" => {
                if args.is_empty() {
                    eprintln!("Usage: search <query>");
                    continue;
                }
                match engine.search(args, 10, &[]).await {
                    Ok(results) => {
                        if results.is_empty() {
                            println!("No memories found.");
                        } else {
                            for (i, r) in results.iter().enumerate() {
                                println!(
                                    "  {}. [sim: {:.3}, imp: {:.1}] {}",
                                    i + 1,
                                    r.similarity,
                                    r.importance,
                                    r.content
                                );
                            }
                        }
                    }
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            "inspect" | "stats" | "i" => {
                let stats = engine.store.stats().await;
                match stats {
                    Ok(s) => {
                        println!("Total memories: {}", s.total_memories);
                        println!("Average importance: {:.1}", s.avg_importance);
                        for (k, v) in &s.by_type {
                            println!("  {}: {}", k, v);
                        }
                    }
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            _ => {
                eprintln!(
                    "Unknown command: '{}'. Type 'help' for available commands.",
                    cmd
                );
            }
        }
    }

    Ok(())
}
