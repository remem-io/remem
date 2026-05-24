//! Integration tests for the reasoning engine (mock provider).

use rememhq_core::memory::types::{MemoryRecord, MemoryType};
use rememhq_core::providers::mock::{MockEmbeddings, MockProvider};

#[test]
fn test_memory_type_display() {
    assert_eq!(MemoryType::Fact.to_string(), "fact");
    assert_eq!(MemoryType::Procedure.to_string(), "procedure");
    assert_eq!(MemoryType::Preference.to_string(), "preference");
    assert_eq!(MemoryType::Decision.to_string(), "decision");
}

#[test]
fn test_memory_type_from_str() {
    assert_eq!("fact".parse::<MemoryType>().unwrap(), MemoryType::Fact);
    assert_eq!(
        "procedure".parse::<MemoryType>().unwrap(),
        MemoryType::Procedure
    );
    assert_eq!("FACT".parse::<MemoryType>().unwrap(), MemoryType::Fact);
    assert!("invalid".parse::<MemoryType>().is_err());
}

#[test]
fn test_memory_record_builder() {
    let record = MemoryRecord::new("test", MemoryType::Fact)
        .with_importance(8.5)
        .with_tags(vec!["tag1".to_string(), "tag2".to_string()])
        .with_session("session-001")
        .with_ttl(30);

    assert_eq!(record.content, "test");
    assert!((record.importance - 8.5).abs() < 0.01);
    assert_eq!(record.tags.len(), 2);
    assert_eq!(record.source_session, Some("session-001".to_string()));
    assert_eq!(record.ttl_days, Some(30));
}

#[test]
fn test_importance_clamping() {
    let low = MemoryRecord::new("low", MemoryType::Fact).with_importance(0.0);
    assert!((low.importance - 1.0).abs() < 0.01);

    let high = MemoryRecord::new("high", MemoryType::Fact).with_importance(99.0);
    assert!((high.importance - 10.0).abs() < 0.01);
}

#[tokio::test]
async fn test_mock_provider_complete() {
    use rememhq_core::providers::Provider;
    let provider = MockProvider;
    let result = provider
        .complete("test prompt", "test-model")
        .await
        .unwrap();
    assert!(!result.is_empty());
}

#[tokio::test]
async fn test_mock_embeddings() {
    use rememhq_core::providers::EmbeddingProvider;
    let embeddings = MockEmbeddings::new(768);

    let vec = embeddings.embed("hello world").await.unwrap();
    assert_eq!(vec.len(), 768);

    // Same input should produce same output (deterministic)
    let vec2 = embeddings.embed("hello world").await.unwrap();
    assert_eq!(vec, vec2);
}

#[tokio::test]
async fn test_mock_embeddings_batch() {
    use rememhq_core::providers::EmbeddingProvider;
    let embeddings = MockEmbeddings::new(768);

    let texts: Vec<String> = vec![
        "first".to_string(),
        "second".to_string(),
        "third".to_string(),
    ];
    let results = embeddings.embed_batch(&texts).await.unwrap();
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|v| v.len() == 768));
}

#[tokio::test]
async fn test_mock_embeddings_dimension() {
    use rememhq_core::providers::EmbeddingProvider;
    let e384 = MockEmbeddings::new(384);
    assert_eq!(e384.dimension(), 384);

    let e768 = MockEmbeddings::new(768);
    assert_eq!(e768.dimension(), 768);
}

#[test]
fn test_provider_initialization_failures_when_keys_missing() {
    // Clear out env keys temporarily to guarantee failure
    let prev_anthropic = std::env::var("ANTHROPIC_API_KEY");
    let prev_openai = std::env::var("OPENAI_API_KEY");
    let prev_google = std::env::var("GOOGLE_API_KEY");

    std::env::remove_var("ANTHROPIC_API_KEY");
    std::env::remove_var("OPENAI_API_KEY");
    std::env::remove_var("GOOGLE_API_KEY");

    use rememhq_core::providers::anthropic::AnthropicProvider;
    use rememhq_core::providers::google::GoogleProvider;
    use rememhq_core::providers::openai::OpenAIProvider;

    assert!(AnthropicProvider::new(None).is_err());
    assert!(OpenAIProvider::new(None).is_err());
    assert!(GoogleProvider::new(None).is_err());

    // Restore env keys
    if let Ok(k) = prev_anthropic {
        std::env::set_var("ANTHROPIC_API_KEY", k);
    }
    if let Ok(k) = prev_openai {
        std::env::set_var("OPENAI_API_KEY", k);
    }
    if let Ok(k) = prev_google {
        std::env::set_var("GOOGLE_API_KEY", k);
    }
}
