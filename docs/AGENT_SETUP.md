# Agent Setup Guide

> How to connect remem to your AI coding assistant.

remem provides persistent, reasoned memory to any MCP-compatible AI tool. This guide covers setup for each supported agent consumer.

## Prerequisites

1. **Install remem** (choose one):
   ```bash
   # From a release binary
   curl -L https://github.com/remem-io/remem/releases/latest/download/remem-$(uname -s)-$(uname -m) -o remem
   chmod +x remem && sudo mv remem /usr/local/bin/

   # From source
   git clone https://github.com/remem-io/remem
   cd remem && cargo install --path rememhq-cli
   ```

2. **Set at least one LLM provider key** (for reasoning and embeddings):
   ```bash
   export ANTHROPIC_API_KEY="sk-ant-..."    # Anthropic Claude
   # OR
   export OPENAI_API_KEY="sk-..."           # OpenAI
   # OR
   export GOOGLE_API_KEY="AIza..."          # Google Gemini
   ```

3. **Verify installation**:
   ```bash
   remem --help
   ```

## Automated Setup (Recommended)

Use `remem init` to generate the correct config for your tool:

```bash
# Single consumer
remem init claude-code --project my-project
remem init codex       --project my-project
remem init cursor      --project my-project
remem init copilot     --project my-project
remem init gemini-cli  --project my-project
remem init opencode    --project my-project

# All consumers at once
remem init all --project my-project
```

Options:
- `--project <name>` — project name for memory isolation (default: `default`)
- `--binary <path>` — override the binary path in generated configs (default: `remem`)

---

## Manual Setup

### Claude Code

Create `.claude/config.json` (or `.mcp.json`) in your project root:

```json
{
  "mcpServers": {
    "remem": {
      "type": "stdio",
      "command": "remem",
      "args": ["mcp", "--project", "my-project"],
      "env": {
        "ANTHROPIC_API_KEY": "${ANTHROPIC_API_KEY}",
        "OPENAI_API_KEY": "${OPENAI_API_KEY}",
        "GOOGLE_API_KEY": "${GOOGLE_API_KEY}"
      }
    }
  }
}
```

### Codex

Create `.codex/config.json` in your project root:

```json
{
  "mcpServers": {
    "remem": {
      "type": "stdio",
      "command": "remem",
      "args": ["mcp", "--project", "my-project"],
      "env": {
        "ANTHROPIC_API_KEY": "${ANTHROPIC_API_KEY}",
        "OPENAI_API_KEY": "${OPENAI_API_KEY}",
        "GOOGLE_API_KEY": "${GOOGLE_API_KEY}"
      }
    }
  }
}
```

### Cursor

Create `.cursor/mcp.json` in your project root:

```json
{
  "mcpServers": {
    "remem": {
      "type": "stdio",
      "command": "remem",
      "args": ["mcp", "--project", "my-project"],
      "env": {
        "ANTHROPIC_API_KEY": "${ANTHROPIC_API_KEY}",
        "OPENAI_API_KEY": "${OPENAI_API_KEY}",
        "GOOGLE_API_KEY": "${GOOGLE_API_KEY}"
      }
    }
  }
}
```

### GitHub Copilot

Create `.github/copilot/mcp.json` in your project root:

```json
{
  "mcpServers": {
    "remem": {
      "type": "stdio",
      "command": "remem",
      "args": ["mcp", "--project", "my-project"],
      "env": {
        "ANTHROPIC_API_KEY": "${ANTHROPIC_API_KEY}",
        "OPENAI_API_KEY": "${OPENAI_API_KEY}",
        "GOOGLE_API_KEY": "${GOOGLE_API_KEY}"
      }
    }
  }
}
```

### Gemini CLI

Create `.gemini/settings.json` in your project root:

```json
{
  "mcpServers": {
    "remem": {
      "command": "remem",
      "args": ["mcp", "--project", "my-project"],
      "env": {
        "ANTHROPIC_API_KEY": "${ANTHROPIC_API_KEY}",
        "OPENAI_API_KEY": "${OPENAI_API_KEY}",
        "GOOGLE_API_KEY": "${GOOGLE_API_KEY}"
      }
    }
  }
}
```

> **Note:** Gemini CLI does not use a `"type"` field — the `command` field implies stdio transport.

### OpenCode

Create `.opencode/config.json` in your project root:

```json
{
  "mcpServers": {
    "remem": {
      "type": "stdio",
      "command": "remem",
      "args": ["mcp", "--project", "my-project"],
      "env": {
        "ANTHROPIC_API_KEY": "${ANTHROPIC_API_KEY}",
        "OPENAI_API_KEY": "${OPENAI_API_KEY}",
        "GOOGLE_API_KEY": "${GOOGLE_API_KEY}"
      }
    }
  }
}
```

---

## Available MCP Tools

Once connected, your AI assistant will have access to these memory tools:

| Tool | Description |
|---|---|
| `mem_store` | Store a memory with automatic LLM importance scoring |
| `mem_recall` | Guided retrieval with LLM re-ranking and reasoning traces |
| `mem_search` | Fast hybrid vector + keyword search (no LLM re-ranking) |
| `mem_update` | Update an existing memory's content or metadata |
| `mem_forget` | Archive or soft-delete a memory |
| `mem_consolidate` | Extract durable facts from a session's working memory |
| `mem_decay` | Apply importance-weighted decay to all active memories |
| `mem_query_knowledge` | Query the knowledge graph for subject-predicate-object triples |
| `mem_get_entity_context` | Get all known facts about a specific entity |

## Development Mode

If you're building remem from source, use `cargo run` instead of the `remem` binary in your configs:

```json
{
  "mcpServers": {
    "remem": {
      "type": "stdio",
      "command": "cargo",
      "args": ["run", "-q", "-p", "rememhq-mcp"],
      "env": {
        "ANTHROPIC_API_KEY": "${ANTHROPIC_API_KEY}",
        "OPENAI_API_KEY": "${OPENAI_API_KEY}",
        "GOOGLE_API_KEY": "${GOOGLE_API_KEY}"
      }
    }
  }
}
```

## Environment Variables

| Variable | Purpose | Default |
|---|---|---|
| `REMEM_PROVIDER` | Default LLM provider | `anthropic` |
| `REMEM_REASONING_PROVIDER` | Override reasoning provider | Falls back to `REMEM_PROVIDER` |
| `REMEM_EMBEDDING_PROVIDER` | Override embedding provider | Falls back to `REMEM_PROVIDER` |
| `REMEM_DATA_DIR` | Root data directory | `~/.remem` |
| `ANTHROPIC_API_KEY` | Anthropic Claude API key | — |
| `OPENAI_API_KEY` | OpenAI API key | — |
| `GOOGLE_API_KEY` | Google Gemini API key | — |

## Troubleshooting

### MCP server not starting

1. Verify `remem` is on your `PATH`:
   ```bash
   which remem  # Unix/macOS
   where remem  # Windows
   ```

2. Test the MCP server directly:
   ```bash
   echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | remem mcp --project test
   ```

3. Check stderr for error messages — the MCP server logs to stderr to avoid contaminating the JSON-RPC channel on stdout.

### Agent not finding tools

- Restart your AI assistant after adding the config file.
- Some tools require a project reload or IDE restart to pick up new MCP servers.
- Verify the config file is in the correct location for your tool (see manual setup above).

### Provider authentication errors

- Ensure at least one API key is set in your environment or in the config's `env` block.
- Test your key directly: `ANTHROPIC_API_KEY=sk-ant-... remem store "test memory"`
