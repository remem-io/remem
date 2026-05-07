# Cloud Provider Integration Guide

## Supported Providers

### Anthropic (Claude)
**Models:** claude-sonnet-4-5 (reasoning), claude-haiku-4-5 (scoring)

```bash
export ANTHROPIC_API_KEY=sk-ant-...
export REMEM_PROVIDER=anthropic
```

### OpenAI
**Models:** gpt-4o (reasoning), gpt-4o-mini (scoring)
**Embeddings:** text-embedding-3-small (768 dimensions)

```bash
export OPENAI_API_KEY=sk-...
export REMEM_PROVIDER=openai
```

### Google (Gemini) — v0.2+
**Models:** gemini-2.0-flash (reasoning + scoring)

```bash
export GOOGLE_AI_API_KEY=...
export REMEM_PROVIDER=google
```

### Local (llama.cpp) — v0.3+
**Models:** phi-3-mini Q4_K_M

```bash
export REMEM_PROVIDER=local
export REMEM_LOCAL_MODEL_PATH=~/.remem/models/phi-3-mini.Q4_K_M.gguf
```

## Configuration

Providers can be configured via environment variables or `.remem/config.toml`:

```toml
[reasoning]
provider = "anthropic"
reasoning_model = "claude-sonnet-4-5"
scoring_model = "claude-haiku-4-5"
```

## Adding a New Provider

1. Implement the `Provider` trait in `rememhq-core/src/providers/`
2. Add the provider variant to the config enum
3. Register the provider in the engine initialization (MCP server, REST API, CLI)
4. Add integration tests in `evals/`
