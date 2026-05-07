//! Integration tests for RememConfig loading and defaults.

use rememhq_core::config::RememConfig;
use std::path::PathBuf;

#[test]
fn test_default_config() {
    let config = RememConfig::default();
    assert_eq!(config.project, "default");
    assert_eq!(config.server.port, 7474);
    assert!(config.memory.working_memory_tokens > 0);
}

#[test]
fn test_config_load_no_file() {
    // Loading without a project dir should return defaults
    let config = RememConfig::load("test-project", None).unwrap();
    assert_eq!(config.project, "test-project");
}

#[test]
fn test_config_load_missing_dir() {
    let fake_dir = PathBuf::from("/nonexistent/path");
    let config = RememConfig::load("test-project", Some(&fake_dir)).unwrap();
    assert_eq!(config.project, "test-project");
}

#[test]
fn test_config_db_path() {
    let config = RememConfig::load("myproject", None).unwrap();
    let db_path = config.db_path();
    assert!(db_path.ends_with("myproject/remem.db"));
}

#[test]
fn test_config_index_path() {
    let config = RememConfig::load("myproject", None).unwrap();
    let idx_path = config.index_path();
    assert!(idx_path.ends_with("myproject/hnsw.idx"));
}

#[test]
fn test_config_project_data_dir() {
    let config = RememConfig::load("alpha", None).unwrap();
    let data_dir = config.project_data_dir();
    let path_str = data_dir.to_string_lossy();
    assert!(path_str.contains("projects"));
    assert!(path_str.contains("alpha"));
}
