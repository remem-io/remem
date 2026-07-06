//! rememhq-core — reasoning memory layer for AI agents.

extern crate libremem_sys;

pub mod config;
pub mod ffi;
pub mod harness;
pub mod loops;
pub mod memory;
pub mod models;
pub mod providers;
pub mod reasoning;
pub mod storage;

pub use config::RememConfig;
pub use memory::types::{MemoryRecord, MemoryResult, MemoryType};
pub use providers::{EmbeddingProvider, Provider};
pub use storage::MemoryStore;
