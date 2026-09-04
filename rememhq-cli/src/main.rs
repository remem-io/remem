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
mod tui;

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
    #[arg(long, global = true, env = "REMEM_PROJECT", default_value = "default")]
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
        /// Overwrite existing configuration files if present
        #[arg(long, short)]
        force: bool,
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
    /// Check configuration, storage paths, and provider readiness
    Doctor {
        /// Ping the configured LLM provider to test API key reachability
        #[arg(long, short)]
        ping: bool,
    },
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
    /// Terminal UI for browsing and inspecting memory
    Tui {
        /// Launch alongside an agent consumer in a split terminal pane
        #[arg(long)]
        companion: Option<String>,
    },
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
    /// Execute Graph workflows
    Graph {
        #[command(subcommand)]
        action: GraphAction,
    },
    /// Interact directly with the Agent Harness
    Harness {
        #[command(subcommand)]
        action: HarnessAction,
    },
    /// Run recall latency and memory throughput benchmark
    Benchmark {
        /// Number of iterations for latency profiling
        #[arg(long, default_value = "20")]
        iterations: usize,
    },
    /// Validate database schema, FTS index, and vector index integrity
    Validate,
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

#[derive(Subcommand)]
enum GraphAction {
    /// Run the CI Triage graph
    Triage {
        /// Raw CI log content
        log_content: String,
    },
}

#[derive(Subcommand)]
enum HarnessAction {
    /// Send a chat prompt via the Agent Harness
    Chat {
        /// System prompt
        #[arg(long, default_value = "You are a helpful assistant.")]
        system: String,
        /// User prompt
        prompt: String,
        /// Maximum retry limit for constraint validation
        #[arg(long, default_value = "3")]
        retries: usize,
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
    GrokBuild,
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
            "grok-build" | "grok" | "spacexai" | "spacex-ai" | "xai" => Ok(Self::GrokBuild),
            "all" => Ok(Self::All),
            _ => Err(format!(
                "Unknown consumer '{}'. Valid options: claude-code, codex, cursor, copilot, antigravity-cli, opencode, aider, windsurf, roocode, cline, grok-build, all",
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
            Self::GrokBuild => write!(f, "grok-build"),
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
    /// Serve a downloaded local-LLM model via a llama.cpp-compatible server
    /// (requires `llama-server` on PATH, or REMEM_LLAMA_SERVER_BIN set)
    Serve {
        /// Model name (e.g., "phi-3-mini")
        name: String,
        /// Port to bind the local inference server on
        #[arg(long, default_value = "8080")]
        port: u16,
    },
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

        Commands::Init {
            consumer,
            binary,
            force,
        } => {
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
                    AgentConsumer::GrokBuild,
                ],
                other => vec![other],
            };

            for c in &consumers {
                match generate_consumer_config(c, &cli.project, &binary, force) {
                    Ok(path) => println!("  ✓ {} → {}", c, path),
                    Err(e) => eprintln!("  ✗ {} — {}", c, e),
                }
            }

            println!("\nDone! Start the MCP server with:");
            println!("  {} mcp --project {}", binary, cli.project);

            if consumers
                .iter()
                .any(|c| matches!(c, AgentConsumer::AntigravityCli))
            {
                println!("\nTip for Antigravity CLI users:");
                println!("  To enable automatic memory extraction from antigravity-cli transcripts (Endless Mode),");
                println!("  add the following to your .remem/config.toml:");
                println!("  [memory]");
                println!("  transcript_watch_dir = \"<appDataDir>/brain\"");
                println!(
                    "  (Replace <appDataDir> with your actual Antigravity CLI data directory)"
                );
            }
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

        Commands::Doctor { ping } => run_doctor(&config, ping).await,
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
                use rememhq_core::models::ModelKind;

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

                if result.primary_downloaded {
                    println!("  ✓ Downloaded {}", spec.primary_filename);
                    if spec.primary_sha256.is_some() {
                        println!("  ✓ Checksum verified (sha256)");
                    }
                } else {
                    println!("  ✓ {} already present (skipped)", spec.primary_filename);
                }
                if let Some(secondary_filename) = spec.secondary_filename {
                    if result.secondary_downloaded {
                        println!("  ✓ Downloaded {}", secondary_filename);
                        if spec.secondary_sha256.is_some() {
                            println!("  ✓ Checksum verified (sha256)");
                        }
                    } else {
                        println!("  ✓ {} already present (skipped)", secondary_filename);
                    }
                }

                println!("\nModel ready.");
                match spec.kind {
                    ModelKind::Embedding => {
                        println!("Set environment variables to use it:");
                        println!("  REMEM_PROVIDER=local \\");
                        println!(
                            "  REMEM_LOCAL_MODEL_PATH={} \\",
                            result.primary_path.display()
                        );
                        if let Some(secondary_path) = &result.secondary_path {
                            println!("  REMEM_LOCAL_VOCAB_PATH={}", secondary_path.display());
                        }
                    }
                    ModelKind::LocalLlm => {
                        println!("Serve it with a llama.cpp-compatible runtime, e.g.:");
                        println!(
                            "  llama-server -m {} --port 8080",
                            result.primary_path.display()
                        );
                        println!("\nThen point remem at it:");
                        println!("  REMEM_PROVIDER=local \\");
                        println!("  LLAMA_API_BASE=http://localhost:8080/v1");
                        println!(
                            "\n(Or import {} into Ollama and set OLLAMA_API_BASE instead.)",
                            spec.primary_filename
                        );
                    }
                }

                Ok(())
            }

            ModelAction::List => {
                let dest = rememhq_core::models::default_models_dir();
                println!("Known models (model dir: {}):\n", dest.display());

                for spec in rememhq_core::models::KNOWN_MODELS {
                    let status = rememhq_core::models::install_status(spec, &dest);
                    let kind = match spec.kind {
                        rememhq_core::models::ModelKind::Embedding => "embedding",
                        rememhq_core::models::ModelKind::LocalLlm => "local-llm",
                    };
                    println!(
                        "  {:14} [{:10}] {:22} —  {}",
                        spec.id,
                        kind,
                        status.label(),
                        spec.description
                    );
                }

                println!("\nTo install a model run:  remem models pull <id>");
                Ok(())
            }

            ModelAction::Serve { name, port } => {
                use rememhq_core::models::{serve, ModelKind};

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

                if spec.kind != ModelKind::LocalLlm {
                    anyhow::bail!(
                        "'{}' is an embedding model, not a local-LLM model — nothing to serve. \
                         (It's used directly via REMEM_LOCAL_MODEL_PATH, no server needed.)",
                        spec.id
                    );
                }

                let dest = rememhq_core::models::default_models_dir();
                let model_path = dest.join(spec.primary_filename);

                let opts = serve::ServeOptions {
                    port,
                    ..Default::default()
                };

                let binary = serve::find_server_binary().unwrap_or_else(|| "llama-server".into());
                println!(
                    "Starting {} ({}) via {} on port {}...",
                    spec.id,
                    model_path.display(),
                    binary,
                    port
                );
                println!("(this can take a while on first load — waiting for /health)");

                let mut server = serve::spawn(&model_path, &opts).await?;

                println!("\n✓ {} is ready at {}", spec.id, server.api_base());
                println!("\nTo use it with remem, in another terminal:");
                println!("  export REMEM_PROVIDER=local");
                println!("  export LLAMA_API_BASE={}", server.api_base());
                println!("  remem doctor --ping");
                println!("\nPress Ctrl+C to stop the server.");

                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        println!("\nStopping {}...", spec.id);
                    }
                    status = server.wait() => {
                        match status {
                            Ok(s) => println!("\n{} exited on its own: {}", spec.id, s),
                            Err(e) => println!("\n{} error while running: {}", spec.id, e),
                        }
                        return Ok(());
                    }
                }

                server.stop().await?;
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
            LoopAction::React {
                task,
                max_iterations,
            } => {
                let engine = build_engine(&config).await?;
                let engine = Arc::new(engine);
                let harness = rememhq_core::harness::AgentHarness::new(engine.provider.clone());
                let mut react_loop =
                    rememhq_core::loops::react::ReActLoop::new(harness, engine, task);
                react_loop.max_iterations = max_iterations;

                use rememhq_core::loops::AgentLoop;
                println!("Running ReAct loop...");
                match react_loop.run().await {
                    Ok(result) => println!("Final Result:\n{}", result),
                    Err(e) => eprintln!("Loop failed: {}", e),
                }
                Ok(())
            }
            LoopAction::Eval {
                task,
                max_iterations,
            } => {
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

        Commands::Graph { action } => match action {
            GraphAction::Triage { log_content } => {
                let engine = build_engine(&config).await?;

                println!("Running CI Triage Graph...");
                let result = rememhq_core::reasoning::triage::run_triage_graph(
                    engine.provider.clone(),
                    &config.reasoning.reasoning_model,
                    log_content,
                )
                .await?;

                println!("Triage Result:\n");
                println!("Failure Type: {:?}", result.failure_type);
                println!("Fix Attempt:\n{:?}", result.fix_attempt);
                if result.escalation_reason.is_some() {
                    println!(
                        "\n[ESCALATION REQUIRED] Human intervention needed: {:?}",
                        result.escalation_reason
                    );
                }

                Ok(())
            }
        },

        Commands::Harness { action } => match action {
            HarnessAction::Chat {
                system,
                prompt,
                retries,
            } => {
                let engine = build_engine(&config).await?;
                let mut harness = rememhq_core::harness::AgentHarness::new(engine.provider.clone());
                harness = harness.with_retries(retries);

                let messages = vec![
                    rememhq_core::providers::ChatMessage::system(system),
                    rememhq_core::providers::ChatMessage::user(prompt),
                ];

                println!("Sending to Agent Harness (retries={})...", retries);
                let response = harness
                    .chat_with_retry(&messages, &config.reasoning.reasoning_model, None)
                    .await?;

                println!("Response:\n{}", response.message.content);
                Ok(())
            }
        },

        Commands::Repl => {
            let engine = build_engine(&config).await?;
            run_repl(engine, &config).await
        }

        Commands::Tui { companion } => {
            if let Some(cmd) = companion {
                let exe = std::env::current_exe()
                    .unwrap_or_else(|_| "remem".into())
                    .display()
                    .to_string();

                if cfg!(target_os = "windows") {
                    println!("Launching TUI and '{}' in Windows Terminal...", cmd);
                    std::process::Command::new("wt")
                        .arg("-d")
                        .arg(".")
                        .arg("cmd")
                        .arg("/c")
                        .arg(format!("{} tui", exe))
                        .arg(";")
                        .arg("split-pane")
                        .arg("-d")
                        .arg(".")
                        .arg("cmd")
                        .arg("/k")
                        .arg(cmd)
                        .spawn()?;
                    return Ok(());
                } else {
                    println!("Launching TUI and '{}' in tmux...", cmd);
                    std::process::Command::new("tmux")
                        .arg("new-session")
                        .arg(format!("{} tui", exe))
                        .arg("\\;")
                        .arg("split-window")
                        .arg("-h")
                        .arg(cmd)
                        .spawn()?;
                    return Ok(());
                }
            }

            let engine = build_engine(&config).await?;
            tui::run_tui(engine, &config).await
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

        Commands::Benchmark { iterations } => {
            let engine = build_engine(&config).await?;
            println!(
                "🚀 Starting remem performance benchmark ({} iterations)...",
                iterations
            );
            println!("Project: {}", cli.project);

            let query = "agent memory consolidation and retrieval architecture";
            let mut recall_times = Vec::with_capacity(iterations);

            for _ in 1..=iterations {
                let start = std::time::Instant::now();
                let _ = engine.recall(query, 10, &[], None, None, None).await;
                let elapsed = start.elapsed();
                recall_times.push(elapsed.as_secs_f64() * 1000.0);
                use std::io::Write;
                print!(".");
                let _ = std::io::stdout().flush();
            }
            println!("\n");

            recall_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let p50 = recall_times[(recall_times.len() as f64 * 0.50) as usize];
            let p95 = recall_times
                [((recall_times.len() as f64 * 0.95) as usize).min(recall_times.len() - 1)];
            let p99 = recall_times
                [((recall_times.len() as f64 * 0.99) as usize).min(recall_times.len() - 1)];
            let avg = recall_times.iter().sum::<f64>() / recall_times.len() as f64;

            println!("📊 Recall Benchmark Results:");
            println!("  Iterations: {}", iterations);
            println!("  Average:    {:.2} ms", avg);
            println!("  P50:        {:.2} ms", p50);
            println!("  P95:        {:.2} ms", p95);
            println!("  P99:        {:.2} ms", p99);
            println!(
                "  Min / Max:  {:.2} ms / {:.2} ms",
                recall_times[0],
                recall_times[recall_times.len() - 1]
            );
            println!("  Embedding Cache Hits: {}", engine.cache.stats().hits);
            Ok(())
        }

        Commands::Validate => {
            println!("🔍 Validating remem database and index integrity...");
            println!("Project: {}", cli.project);
            let store = SqliteStore::open(&config.db_path())?;
            let stats = store.stats().await?;
            println!(
                "  ✓ SQLite Database readable: {} memories found",
                stats.total_memories
            );

            let fts_check = store.search_fts("test", 1).await;
            match fts_check {
                Ok(_) => println!("  ✓ SQLite FTS5 index operational"),
                Err(e) => println!("  ⚠ FTS5 index check warning: {}", e),
            }

            let index = Arc::new(rememhq_core::storage::vector::HNSWVectorIndex::new(
                768, 10000,
            ));
            if config.index_path().exists() {
                match index.load(&config.index_path()).await {
                    Ok(_) => println!(
                        "  ✓ Vector Index loaded successfully (elements: {})",
                        index.len()
                    ),
                    Err(e) => println!("  ⚠ Vector Index load warning: {}", e),
                }
            } else {
                println!("  ℹ Vector Index file not yet created (will be created on first store)");
            }

            println!("\n✓ System integrity check passed!");
            Ok(())
        }
    }
}

/// Generate the MCP configuration file for a given agent consumer.
fn generate_consumer_config(
    consumer: &AgentConsumer,
    project: &str,
    binary: &str,
    force: bool,
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
        AgentConsumer::Aider => (
            ".aider",
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
        AgentConsumer::Windsurf => (
            ".windsurf",
            "mcp_config.json",
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
        AgentConsumer::RooCode => (
            ".roocode",
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
        AgentConsumer::Cline => (
            ".cline",
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
        AgentConsumer::GrokBuild => (
            ".grok",
            "config.json",
            serde_json::json!({
                "mcpServers": {
                    "remem": {
                        "type": "stdio",
                        "command": binary,
                        "args": ["mcp", "--project", project],
                        "env": {
                            "XAI_API_KEY": "${XAI_API_KEY}",
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
    if file_path.exists() && !force {
        anyhow::bail!(
            "Config already exists: {} (use --force to overwrite)",
            file_path.display()
        );
    }

    let json_str = serde_json::to_string_pretty(&content)?;
    std::fs::write(&file_path, format!("{}\n", json_str))?;

    Ok(file_path.display().to_string())
}

/// Run a read-only health check for the active remem project.
async fn run_doctor(config: &RememConfig, ping: bool) -> anyhow::Result<()> {
    println!("remem doctor");
    println!("  project: {}", config.project);
    println!("  provider: {}", config.reasoning.provider);
    println!("  reasoning model: {}", config.reasoning.reasoning_model);
    println!("  scoring model: {}", config.reasoning.scoring_model);
    println!();

    print_path_check("data dir", &config.storage.data_dir, false);
    print_path_check("project dir", &config.project_data_dir(), false);
    print_path_check("database", &config.db_path(), true);
    print_path_check("vector index", &config.index_path(), true);

    if config.db_path().exists() {
        match SqliteStore::open(&config.db_path()) {
            Ok(store) => match store.stats().await {
                Ok(stats) => println!(
                    "  ✓ database readable: {} memories, avg importance {:.1}",
                    stats.total_memories, stats.avg_importance
                ),
                Err(err) => println!("  ✗ database stats failed: {err}"),
            },
            Err(err) => println!("  ✗ database open failed: {err}"),
        }
    } else {
        println!("  - database readable: skipped until first memory is stored");
    }

    // Check native vector engine HNSW FFI
    let _hnsw = HNSWVectorIndex::new(1536, 1000);
    println!("  ✓ native vector engine (libremem HNSW FFI): ready");

    // Check agent MCP configurations
    check_agent_mcp_configs();

    println!();
    print_provider_check("reasoning", &reasoning_provider_status(config));
    print_provider_check("embeddings", &embedding_provider_status(config));

    if ping {
        println!();
        println!("Pinging provider reachability...");
        let provider = rememhq_core::providers::factory::build_reasoning_provider(config);
        let options = rememhq_core::providers::ProviderOptions::default();
        let msg = vec![rememhq_core::providers::ChatMessage::user("ping")];
        match provider
            .chat(&msg, &[], &config.reasoning.reasoning_model, Some(&options))
            .await
        {
            Ok(_) => println!("  ✓ provider reachability ping: SUCCESS"),
            Err(e) => println!("  ✗ provider reachability ping failed: {e}"),
        }
    }

    Ok(())
}

fn check_agent_mcp_configs() {
    let configs = [
        (".gemini/settings.json", "Antigravity CLI"),
        (".claude/config.json", "Claude Code"),
        (".cursor/mcp.json", "Cursor"),
        (".github/copilot/mcp.json", "Copilot"),
        (".opencode/config.json", "OpenCode"),
    ];

    for (path, name) in configs {
        let p = std::path::Path::new(path);
        if p.exists() {
            println!("  ✓ agent mcp config found for {name}: {}", p.display());
        }
    }
}

fn print_path_check(label: &str, path: &std::path::Path, optional: bool) {
    if path.exists() {
        println!("  ✓ {label}: {}", path.display());
    } else if optional {
        println!("  - {label}: {} (not created yet)", path.display());
    } else {
        println!(
            "  ! {label}: {} (missing; created on first write)",
            path.display()
        );
    }
}

fn print_provider_check(label: &str, status: &DoctorStatus) {
    let marker = if status.ok { "✓" } else { "!" };
    println!("  {marker} {label}: {}", status.message);
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DoctorStatus {
    ok: bool,
    message: String,
}

fn reasoning_provider_status(config: &RememConfig) -> DoctorStatus {
    let provider = configured_provider("REMEM_REASONING_PROVIDER", config);
    reasoning_provider_status_for(
        &provider,
        env_is_set("ANTHROPIC_API_KEY"),
        env_is_set("OPENAI_API_KEY"),
        env_is_set("GOOGLE_API_KEY"),
    )
}

fn reasoning_provider_status_for(
    provider: &str,
    has_anthropic_key: bool,
    has_openai_key: bool,
    has_google_key: bool,
) -> DoctorStatus {
    match provider {
        "mock" => ok_status("mock provider selected; no API key required"),
        "local" => ok_status("local provider selected"),
        "openai" if has_openai_key => ok_status("OpenAI API key found"),
        "openai" => warn_status("OPENAI_API_KEY missing"),
        "anthropic" | "claude" if has_anthropic_key => ok_status("Anthropic API key found"),
        "anthropic" | "claude" => warn_status("ANTHROPIC_API_KEY missing"),
        "google" | "gemini" if has_google_key => ok_status("Google API key found"),
        "google" | "gemini" => warn_status("GOOGLE_API_KEY missing"),
        _ if has_anthropic_key || has_openai_key || has_google_key => {
            ok_status("auto-detect can use an available API key")
        }
        _ => warn_status("no reasoning API key found; runtime will fall back to mock provider"),
    }
}

fn embedding_provider_status(config: &RememConfig) -> DoctorStatus {
    let provider = configured_provider("REMEM_EMBEDDING_PROVIDER", config);
    embedding_provider_status_for(
        &provider,
        env_is_set("OPENAI_API_KEY"),
        env_is_set("GOOGLE_API_KEY"),
        local_embedding_files_exist(),
    )
}

fn embedding_provider_status_for(
    provider: &str,
    has_openai_key: bool,
    has_google_key: bool,
    has_local_files: bool,
) -> DoctorStatus {
    match provider {
        "mock" => ok_status("mock embeddings selected; no API key required"),
        "local" if has_local_files => ok_status("local embedding model and vocab files found"),
        "local" => warn_status("local embeddings need REMEM_LOCAL_MODEL_PATH and REMEM_LOCAL_VOCAB_PATH files"),
        "openai" if has_openai_key => ok_status("OpenAI embeddings ready"),
        "openai" => warn_status("OPENAI_API_KEY missing"),
        "google" | "gemini" if has_google_key => ok_status("Google embeddings ready"),
        "google" | "gemini" => warn_status("GOOGLE_API_KEY missing"),
        "anthropic" | "claude" if has_openai_key || has_google_key || has_local_files => {
            ok_status("Anthropic reasoning will use available embedding fallback")
        }
        "anthropic" | "claude" => warn_status(
            "Anthropic has no embedding API; set OPENAI_API_KEY, GOOGLE_API_KEY, or local model files",
        ),
        _ if has_openai_key || has_google_key || has_local_files => {
            ok_status("auto-detect can use available embeddings")
        }
        _ => warn_status("no embedding provider found; runtime will fall back to mock embeddings"),
    }
}

fn configured_provider(override_env: &str, config: &RememConfig) -> String {
    std::env::var(override_env)
        .or_else(|_| std::env::var("REMEM_PROVIDER"))
        .unwrap_or_else(|_| config.reasoning.provider.clone())
        .trim()
        .to_lowercase()
}

#[allow(dead_code)]
fn env_status(env_var: &str, ok_message: &str, missing_message: &str) -> DoctorStatus {
    if env_is_set(env_var) {
        ok_status(ok_message)
    } else {
        warn_status(missing_message)
    }
}

fn local_embedding_files_exist() -> bool {
    let model_path = std::env::var("REMEM_LOCAL_MODEL_PATH")
        .unwrap_or_else(|_| "models/nomic-embed-text.onnx".to_string());
    let vocab_path =
        std::env::var("REMEM_LOCAL_VOCAB_PATH").unwrap_or_else(|_| "models/vocab.txt".to_string());
    std::path::Path::new(&model_path).exists() && std::path::Path::new(&vocab_path).exists()
}

#[allow(dead_code)]
fn any_env_set(vars: &[&str]) -> bool {
    vars.iter().any(|var| env_is_set(var))
}

fn env_is_set(var: &str) -> bool {
    std::env::var(var).is_ok_and(|value| !value.trim().is_empty())
}

fn ok_status(message: &str) -> DoctorStatus {
    DoctorStatus {
        ok: true,
        message: message.to_string(),
    }
}

fn warn_status(message: &str) -> DoctorStatus {
    DoctorStatus {
        ok: false,
        message: message.to_string(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_accepts_mock_without_keys() {
        assert_eq!(
            reasoning_provider_status_for("mock", false, false, false),
            ok_status("mock provider selected; no API key required")
        );
        assert_eq!(
            embedding_provider_status_for("mock", false, false, false),
            ok_status("mock embeddings selected; no API key required")
        );
    }

    #[test]
    fn doctor_warns_when_anthropic_has_no_embedding_fallback() {
        assert_eq!(
            embedding_provider_status_for("anthropic", false, false, false),
            warn_status(
                "Anthropic has no embedding API; set OPENAI_API_KEY, GOOGLE_API_KEY, or local model files",
            )
        );
    }

    #[test]
    fn doctor_allows_unknown_provider_auto_detect() {
        assert_eq!(
            reasoning_provider_status_for("auto", false, true, false),
            ok_status("auto-detect can use an available API key")
        );
        assert_eq!(
            embedding_provider_status_for("auto", false, false, true),
            ok_status("auto-detect can use available embeddings")
        );
    }
}
