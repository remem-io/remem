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

mod agent;

use rememhq_core::config::RememConfig;
use rememhq_core::memory::types::{MemoryRecord, MemoryType};
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
    /// Initialize remem config for an AI agent consumer
    Init {
        /// Which agent consumer to configure
        consumer: AgentConsumer,
        /// Override the remem binary path in generated configs
        #[arg(long, default_value = "remem")]
        binary: String,
    },
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
    /// AI Companion Terminal
    Agent,
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
    /// Project management
    Projects {
        #[command(subcommand)]
        action: ProjectAction,
    },
    /// Session management
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },
    /// Run an agent loop
    Loop {
        #[command(subcommand)]
        action: LoopAction,
    },
    /// Get project context
    Context {
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Forget a memory by ID
    Forget { id: String },
}

#[derive(Subcommand)]
enum SessionAction {
    /// Compress a session transcript into durable facts
    Compress { session_id: String },
}

#[derive(Subcommand)]
enum LoopAction {
    /// Run a ReAct loop
    React {
        task: String,
        #[arg(long, default_value = "5")]
        max_iterations: usize,
    },
    /// Run a Generate-Evaluate-Refine loop
    Eval {
        task: String,
        #[arg(long, default_value = "5")]
        max_iterations: usize,
    },
}

/// Supported AI agent consumers for `remem init`.
#[derive(Clone, Debug)]
enum AgentConsumer {
    ClaudeCode,
    Codex,
    Cursor,
    Copilot,
    AntigravityCli,
    OpenCode,
    Aider,
    Windsurf,
    RooCode,
    Cline,
    All,
}

impl std::str::FromStr for AgentConsumer {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "claude-code" | "claude" => Ok(Self::ClaudeCode),
            "codex" => Ok(Self::Codex),
            "cursor" => Ok(Self::Cursor),
            "copilot" | "github-copilot" => Ok(Self::Copilot),
            "antigravity-cli" | "gemini" => Ok(Self::AntigravityCli),
            "opencode" => Ok(Self::OpenCode),
            "aider" => Ok(Self::Aider),
            "windsurf" => Ok(Self::Windsurf),
            "roocode" | "roo-code" => Ok(Self::RooCode),
            "cline" => Ok(Self::Cline),
            "all" => Ok(Self::All),
            _ => Err(format!(
                "Unknown consumer '{}'. Valid options: claude-code, codex, cursor, copilot, antigravity-cli, opencode, aider, windsurf, roocode, cline, all",
                s
            )),
        }
    }
}

impl std::fmt::Display for AgentConsumer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClaudeCode => write!(f, "claude-code"),
            Self::Codex => write!(f, "codex"),
            Self::Cursor => write!(f, "cursor"),
            Self::Copilot => write!(f, "copilot"),
            Self::AntigravityCli => write!(f, "antigravity-cli"),
            Self::OpenCode => write!(f, "opencode"),
            Self::Aider => write!(f, "aider"),
            Self::Windsurf => write!(f, "windsurf"),
            Self::RooCode => write!(f, "roocode"),
            Self::Cline => write!(f, "cline"),
            Self::All => write!(f, "all"),
        }
    }
}

#[derive(Subcommand)]
enum ProjectAction {
    /// List all projects
    List,
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

        Commands::Init { consumer, binary } => {
            let consumers = match consumer {
                AgentConsumer::All => vec![
                    AgentConsumer::ClaudeCode,
                    AgentConsumer::Codex,
                    AgentConsumer::Cursor,
                    AgentConsumer::Copilot,
                    AgentConsumer::AntigravityCli,
                    AgentConsumer::OpenCode,
                    AgentConsumer::Aider,
                    AgentConsumer::Windsurf,
                    AgentConsumer::RooCode,
                    AgentConsumer::Cline,
                ],
                other => vec![other],
            };

            for c in &consumers {
                match generate_consumer_config(c, &cli.project, &binary) {
                    Ok(path) => println!("  ✓ {} → {}", c, path),
                    Err(e) => eprintln!("  ✗ {} — {}", c, e),
                }
            }

            println!("\nDone! Start the MCP server with:");
            println!("  {} mcp --project {}", binary, cli.project);
            Ok(())
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

            let stored = engine.store_memory(record, auto_score, None).await?;
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
            let results = engine.recall(&query, limit, &[], None, None, None).await?;

            if results.is_empty() {
                println!("No memories found for: \"{}\"", query);
            } else {
                println!("Found {} memories:\n", results.len());
                for (i, r) in results.into_iter().enumerate() {
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
            let results = engine.search(&query, limit, &[], None).await?;

            if results.is_empty() {
                println!("No memories found for: \"{}\"", query);
            } else {
                println!("Found {} memories:\n", results.len());
                for (i, r) in results.into_iter().enumerate() {
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
                println!(
                    "  (approx. {:.0} MB)",
                    spec.approx_bytes as f64 / 1_000_000.0
                );

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
                println!("  REMEM_PROVIDER=local \\");
                println!("  REMEM_LOCAL_MODEL_PATH={} \\", result.onnx_path.display());
                println!("  REMEM_LOCAL_VOCAB_PATH={}", result.vocab_path.display());

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

        Commands::Projects { action } => match action {
            ProjectAction::List => {
                let projects_dir = config.storage.data_dir.join("projects");
                println!("Projects (data dir: {}):\n", projects_dir.display());

                if !projects_dir.exists() {
                    println!("  No projects found.");
                    return Ok(());
                }

                let mut count = 0;
                let mut entries: Vec<_> = std::fs::read_dir(&projects_dir)?
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect();

                entries.sort();

                for project_name in entries {
                    // Simple check if it's a valid project directory
                    let project_dir = projects_dir.join(&project_name);
                    let db_exists = project_dir.join("remem.db").exists();
                    if db_exists {
                        println!("  - {}", project_name);
                        count += 1;
                    }
                }

                if count == 0 {
                    println!("  No projects found.");
                } else {
                    println!("\nTotal: {} project(s)", count);
                }
                Ok(())
            }
        },

        Commands::Session { action } => match action {
            SessionAction::Compress { session_id } => {
                let engine = build_engine(&config).await?;
                println!("Compressing session '{}' into durable facts...", session_id);
                let report = engine
                    .compress_session_transcript(&session_id, None)
                    .await?;
                println!("✓ Session compressed successfully!");
                println!("  New facts created: {}", report.new_facts);
                println!("  Contradictions resolved: {}", report.contradictions.len());
                // Save index
                engine.index.save(&config.index_path()).await?;
                Ok(())
            }
        },

        Commands::Context { limit } => {
            let engine = build_engine(&config).await?;
            let memories = engine.store.list(&[], None, None, limit).await?;
            if memories.is_empty() {
                println!("No context available for project '{}'.", cli.project);
            } else {
                println!("Project Context (Top {} memories):\n", limit);
                for (i, m) in memories.into_iter().enumerate() {
                    println!("{}. [{}] {}", i + 1, m.memory_type, m.content);
                }
            }
            Ok(())
        }

        Commands::Loop { action } => match action {
            LoopAction::React { task, max_iterations } => {
                let engine = build_engine(&config).await?;
                let engine = Arc::new(engine);
                let harness = rememhq_core::harness::AgentHarness::new(engine.provider.clone());
                let mut react_loop = rememhq_core::loops::react::ReActLoop::new(harness, engine, task);
                react_loop.max_iterations = max_iterations;
                
                use rememhq_core::loops::AgentLoop;
                println!("Running ReAct loop...");
                match react_loop.run().await {
                    Ok(result) => println!("Final Result:\n{}", result),
                    Err(e) => eprintln!("Loop failed: {}", e),
                }
                Ok(())
            }
            LoopAction::Eval { task, max_iterations } => {
                let engine = build_engine(&config).await?;
                let harness = rememhq_core::harness::AgentHarness::new(engine.provider.clone());
                let mut eval_loop = rememhq_core::loops::eval::GenerateEvaluateRefineLoop::new(
                    harness,
                    task,
                    config.reasoning.reasoning_model.clone(),
                    config.reasoning.reasoning_model.clone(),
                );
                eval_loop.max_iterations = max_iterations;

                use rememhq_core::loops::AgentLoop;
                println!("Running Generate-Evaluate-Refine loop...");
                match eval_loop.run().await {
                    Ok(result) => println!("Final Result:\n{}", result),
                    Err(e) => eprintln!("Loop failed: {}", e),
                }
                Ok(())
            }
        },

        Commands::Forget { id } => {
            let engine = build_engine(&config).await?;
            let uuid = uuid::Uuid::parse_str(&id)?;
            let success = engine
                .forget(uuid, rememhq_core::memory::types::ForgetMode::Archive)
                .await?;
            if success {
                println!("✓ Archived memory {}", id);
                engine.index.save(&config.index_path()).await?;
            } else {
                println!("Memory {} not found or could not be archived.", id);
            }
            Ok(())
        }

        Commands::Repl => {
            let engine = build_engine(&config).await?;
            run_repl(engine, &config).await
        }

        Commands::Agent => {
            let engine = build_engine(&config).await?;
            agent::run_agent(engine, &config).await
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
                        match engine.store_memory(record, auto_score, None).await {
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

/// Generate the MCP configuration file for a given agent consumer.
fn generate_consumer_config(
    consumer: &AgentConsumer,
    project: &str,
    binary: &str,
) -> anyhow::Result<String> {
    let (dir_path, file_name, content) = match consumer {
        AgentConsumer::ClaudeCode => (
            ".claude",
            "config.json",
            serde_json::json!({
                "mcpServers": {
                    "remem": {
                        "type": "stdio",
                        "command": binary,
                        "args": ["mcp", "--project", project],
                        "env": {
                            "ANTHROPIC_API_KEY": "${ANTHROPIC_API_KEY}",
                            "OPENAI_API_KEY": "${OPENAI_API_KEY}",
                            "GOOGLE_API_KEY": "${GOOGLE_API_KEY}"
                        }
                    }
                }
            }),
        ),
        AgentConsumer::Codex => (
            ".codex",
            "config.json",
            serde_json::json!({
                "mcpServers": {
                    "remem": {
                        "type": "stdio",
                        "command": binary,
                        "args": ["mcp", "--project", project],
                        "env": {
                            "ANTHROPIC_API_KEY": "${ANTHROPIC_API_KEY}",
                            "OPENAI_API_KEY": "${OPENAI_API_KEY}",
                            "GOOGLE_API_KEY": "${GOOGLE_API_KEY}"
                        }
                    }
                }
            }),
        ),
        AgentConsumer::Cursor => (
            ".cursor",
            "mcp.json",
            serde_json::json!({
                "mcpServers": {
                    "remem": {
                        "type": "stdio",
                        "command": binary,
                        "args": ["mcp", "--project", project],
                        "env": {
                            "ANTHROPIC_API_KEY": "${ANTHROPIC_API_KEY}",
                            "OPENAI_API_KEY": "${OPENAI_API_KEY}",
                            "GOOGLE_API_KEY": "${GOOGLE_API_KEY}"
                        }
                    }
                }
            }),
        ),
        AgentConsumer::Copilot => (
            ".github/copilot",
            "mcp.json",
            serde_json::json!({
                "mcpServers": {
                    "remem": {
                        "type": "stdio",
                        "command": binary,
                        "args": ["mcp", "--project", project],
                        "env": {
                            "ANTHROPIC_API_KEY": "${ANTHROPIC_API_KEY}",
                            "OPENAI_API_KEY": "${OPENAI_API_KEY}",
                            "GOOGLE_API_KEY": "${GOOGLE_API_KEY}"
                        }
                    }
                }
            }),
        ),
        AgentConsumer::AntigravityCli => (
            ".gemini",
            "settings.json",
            serde_json::json!({
                "mcpServers": {
                    "remem": {
                        "command": binary,
                        "args": ["mcp", "--project", project],
                        "env": {
                            "ANTHROPIC_API_KEY": "${ANTHROPIC_API_KEY}",
                            "OPENAI_API_KEY": "${OPENAI_API_KEY}",
                            "GOOGLE_API_KEY": "${GOOGLE_API_KEY}"
                        }
                    }
                }
            }),
        ),
        AgentConsumer::OpenCode => (
            ".opencode",
            "config.json",
            serde_json::json!({
                "mcpServers": {
                    "remem": {
                        "type": "stdio",
                        "command": binary,
                        "args": ["mcp", "--project", project],
                        "env": {
                            "ANTHROPIC_API_KEY": "${ANTHROPIC_API_KEY}",
                            "OPENAI_API_KEY": "${OPENAI_API_KEY}",
                            "GOOGLE_API_KEY": "${GOOGLE_API_KEY}"
                        }
                    }
                }
            }),
        ),
        AgentConsumer::All => unreachable!("All is expanded before calling this function"),
    };

    let dir = std::path::Path::new(dir_path);
    std::fs::create_dir_all(dir)?;

    let file_path = dir.join(file_name);
    if file_path.exists() {
        anyhow::bail!("Config already exists: {}", file_path.display());
    }

    let json_str = serde_json::to_string_pretty(&content)?;
    std::fs::write(&file_path, format!("{}\n", json_str))?;

    Ok(file_path.display().to_string())
}

/// Build a reasoning engine from config (shared setup for CLI commands).
///
/// Uses the centralised provider factory for cascading fallbacks.
async fn build_engine(config: &RememConfig) -> anyhow::Result<ReasoningEngine> {
    let store = Arc::new(SqliteStore::open(&config.db_path())?);

    let provider = rememhq_core::providers::factory::build_reasoning_provider(config);
    let embeddings = rememhq_core::providers::factory::build_embedding_provider(config);

    let index = Arc::new(HNSWVectorIndex::new(embeddings.dimension(), 10000));
    let _ = index.load(&config.index_path()).await;

    Ok(ReasoningEngine::new(
        config.clone(),
        provider,
        embeddings,
        store,
        index,
        Vec::new(),
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
                    .store_memory(MemoryRecord::new(args, MemoryType::Fact), true, None)
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
                match engine.recall(args, 8, &[], None, None, None).await {
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
                match engine.search(args, 10, &[], None).await {
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
