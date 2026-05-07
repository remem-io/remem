//! rememhq-core — reasoning memory layer for AI agents.
//!
//! This crate provides the core memory storage, vector indexing, cloud LLM
//! provider integrations, and reasoning engine that power remem.

pub mod config;
pub mod ffi;
pub mod memory;
pub mod providers;
pub mod reasoning;
pub mod storage;

pub use config::RememConfig;
pub use memory::types::{MemoryRecord, MemoryResult, MemoryType};
pub use providers::{EmbeddingProvider, Provider};
pub use storage::MemoryStore;
