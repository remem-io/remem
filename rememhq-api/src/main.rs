//! remem REST API server built with Axum.
//!
//! Endpoints mirror the MCP tools:
//! - POST   /v1/memories              → mem_store
//! - GET    /v1/memories/recall       → mem_recall
//! - GET    /v1/memories/search       → mem_search
//! - GET    /v1/memories/:id          → get_memory
//! - PATCH  /v1/memories/:id          → mem_update
//! - DELETE /v1/memories/:id          → mem_forget
//! - POST   /v1/sessions/:id/consolidate → mem_consolidate
//! - GET    /v1/knowledge             → query_knowledge
//! - GET    /v1/knowledge/entity/:name → get_entity_context
//! - GET    /v1/stats                 → get_stats

mod cors;
pub mod cursor;
pub mod handlers;
mod middleware;
pub mod models;
pub mod openapi;
mod router;
mod routes;

use clap::Parser;
use std::sync::Arc;

use rememhq_core::config::RememConfig;
use rememhq_core::storage::sqlite::SqliteStore;
use rememhq_core::storage::vector::{HNSWVectorIndex, VectorIndex};

#[derive(Parser)]
pub struct Args {
    #[arg(long, default_value = "7474")]
    pub port: u16,
    #[arg(long, default_value = "default")]
    pub project: String,
    /// Allowed CORS origin(s), e.g. "http://localhost:3000" or "*".
    /// Can also be set via REMEM_CORS_ORIGIN env var.
    /// Defaults to local origins (http://localhost, http://127.0.0.1).
    #[arg(long)]
    pub cors_origin: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter("rememhq=info,tower_http=debug")
        .init();

    let args = Args::parse();
    let config = RememConfig::load(&args.project, None)?;

    // Initialize components
    let store = Arc::new(SqliteStore::open(&config.db_path())?);

    let provider = rememhq_core::providers::factory::build_reasoning_provider(&config);
    let embeddings = rememhq_core::providers::factory::build_embedding_provider(&config);

    tracing::info!(
        "Initializing ReasoningEngine with project: {}",
        args.project
    );
    tracing::info!("Using reasoning provider: {}", provider.name());
    tracing::info!("Using embedding provider (dim={})", embeddings.dimension());

    let index = Arc::new(HNSWVectorIndex::new(embeddings.dimension(), 10000));
    let _ = index.load(&config.index_path()).await;

    let engine = Arc::new(
        rememhq_core::reasoning::EngineBuilder::from_config(config.clone())
            .with_provider(provider)
            .with_embeddings(embeddings)
            .with_store(store)
            .with_index(index)
            .build()
            .await?,
    );

    let rate_limit_state = Arc::new(tokio::sync::Mutex::new(
        middleware::rate_limit::RateLimiterState::new(),
    ));

    // Start background tasks
    let bg_engine = engine.clone();
    let decay_hours = config.memory.importance_decay_interval_hours as u64;
    tokio::spawn(async move {
        let decay_interval = std::time::Duration::from_secs(decay_hours * 3600);
        let mut decay_timer = tokio::time::interval(decay_interval);
        let mut ttl_timer = tokio::time::interval(std::time::Duration::from_secs(3600)); // check TTL every hour

        loop {
            tokio::select! {
                _ = decay_timer.tick() => {
                    tracing::info!("Running background memory decay...");
                    let _ = bg_engine.apply_decay(0.9).await;
                    let _ = bg_engine.save_index().await;
                }
                _ = ttl_timer.tick() => {
                    tracing::info!("Running background TTL expiration...");
                    let _ = bg_engine.expire_ttl().await;
                    let _ = bg_engine.save_index().await;
                }
            }
        }
    });

    let cors_origin = args
        .cors_origin
        .or_else(|| std::env::var("REMEM_CORS_ORIGIN").ok());

    let app = router::build_app(engine, rate_limit_state, cors_origin.as_deref());

    let auth_enabled = std::env::var("REMEM_API_KEY")
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false);

    if !auth_enabled {
        tracing::warn!(
            "⚠️  REMEM_API_KEY is unset! API server running in unauthenticated mode (dev mode). All requests will be allowed."
        );
    } else {
        tracing::info!("Bearer token authentication enabled via REMEM_API_KEY.");
    }

    let addr = format!("0.0.0.0:{}", args.port);
    tracing::info!("remem REST API listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("Shutdown signal received, starting graceful shutdown...");
}
