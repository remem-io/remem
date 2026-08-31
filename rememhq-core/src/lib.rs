//! rememhq-core — reasoning memory layer for AI agents.

extern crate libremem_sys;

pub mod config;
pub mod context;
pub mod ffi;
pub mod graph;
pub mod harness;
pub mod loops;
pub mod memory;
pub mod models;
pub mod providers;
pub mod reasoning;
pub mod safety;
pub mod session;
pub mod storage;
pub mod telemetry;

pub use config::RememConfig;
pub use memory::types::{MemoryRecord, MemoryResult, MemoryType};
pub use providers::{EmbeddingCache, EmbeddingProvider, Provider, ProviderPool};
pub use safety::{SafetyAssessment, SafetyGuard};
pub use storage::MemoryStore;
pub use telemetry::{MemoryMetrics, MetricsSnapshot};
