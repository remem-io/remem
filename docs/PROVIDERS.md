# Cloud Provider Integration Guide

## Supported Providers

### Anthropic (Claude)

**Reasoning model:** `claude-sonnet-4-5`
**Scoring model:** `claude-haiku-4-5`

```bash
export ANTHROPIC_API_KEY=sk-ant-...
export REMEM_PROVIDER=anthropic
```

### OpenAI

**Reasoning model:** `gpt-4o`
**Scoring model:** `gpt-4o-mini`
**Embeddings:** `text-embedding-3-small` (768 dimensions)

```bash
export OPENAI_API_KEY=sk-...
export REMEM_PROVIDER=openai
```

### Google (Gemini)

**Reasoning model:** `gemini-2.0-flash` (reasoning + scoring)
**Embeddings:** `text-embedding-004` (768 dimensions)

```bash
export GOOGLE_API_KEY=...
export REMEM_PROVIDER=google
```

> **Note:** The environment variable is `GOOGLE_API_KEY` (not `GOOGLE_AI_API_KEY`).

### Local (llama.cpp / Ollama) — v0.3+

**Models:** any OpenAI-compatible endpoint (phi-3-mini, llama-3, etc.)

```bash
export REMEM_PROVIDER=local
export OLLAMA_API_BASE=http://localhost:11434/v1
# or
export LLAMA_API_BASE=http://localhost:8080/v1
```

For local embeddings (v0.2+):

```bash
# Pull the embedding model first
remem models pull nomic-embed

export REMEM_LOCAL_MODEL_PATH=~/.remem/models/nomic-embed-text.onnx
export REMEM_LOCAL_VOCAB_PATH=~/.remem/models/vocab.txt
```

## Provider-Aware Model Defaults

When `REMEM_PROVIDER` is set, remem automatically picks the appropriate
default reasoning and scoring models for that provider. You can override
either individually:

| Variable | Purpose | Example |
|---|---|---|
| `REMEM_PROVIDER` | Default provider for all operations | `google` |
| `REMEM_REASONING_PROVIDER` | Override reasoning provider only | `anthropic` |
| `REMEM_EMBEDDING_PROVIDER` | Override embedding provider only | `openai` |
| `REMEM_REASONING_MODEL` | Override reasoning model name | `gemini-1.5-pro` |
| `REMEM_SCORING_MODEL` | Override scoring model name | `gemini-2.0-flash-lite` |

## Configuration via TOML

Providers can be configured in `.remem/config.toml` in your project directory:

```toml
[reasoning]
provider = "google"
reasoning_model = "gemini-2.0-flash"
scoring_model = "gemini-2.0-flash"
```

## Adding a New Provider

1. Implement the `Provider` trait in `rememhq-core/src/providers/`
2. Implement the `EmbeddingProvider` trait (if the provider offers embeddings)
3. Add the provider variant to the match arms in `rememhq-api/src/main.rs`,
   `rememhq-mcp/src/main.rs`, and `rememhq-cli/src/main.rs`
4. Add provider-aware model defaults to `reasoning_model_for()` and
   `scoring_model_for()` in `rememhq-core/src/config.rs`
5. Add integration tests in `evals/`
