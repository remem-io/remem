# rememhq-mcp

[![Crates.io](https://img.shields.io/crates/v/rememhq-mcp.svg)](https://crates.io/crates/rememhq-mcp)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

The **Model Context Protocol (MCP)** server for **remem** — the reasoning memory layer for AI agents.

This crate provides an MCP-compliant server that exposes remem's powerful memory management capabilities directly to your AI assistants (like Claude Desktop, Cursor, Copilot, or custom agents) through standard stdio JSON-RPC.

## Installation

Install the MCP server globally using cargo:

```bash
cargo install rememhq-mcp
```

## Setup & Configuration

Once installed, you can configure your AI agent or MCP host to launch `rememhq-mcp`. For instance, to use it with Claude Desktop, you can configure your MCP settings:

```json
{
  "mcpServers": {
    "remem": {
      "command": "rememhq-mcp",
      "args": ["--project", "claude-desktop-memory"]
    }
  }
}
```

This immediately equips the agent with native tools to read, write, update, and search its own memory across sessions.

## Provided Tools

When running, the server exposes several MCP tools, including:

- `mem_store`: Save a memory or fact.
- `mem_recall`: Perform semantic recall using LLM orchestration.
- `mem_search`: Rapid nearest-neighbor search.
- `mem_update`: Alter existing facts when context changes.
- `mem_forget`: Delete a fact from the database.
- `mem_consolidate`: Consolidate complex conversational traces into durable memory structures.

## Ecosystem Integration

The `rememhq-mcp` is designed to be fully standalone. However, you can use the `rememhq-cli` tool to automatically configure this MCP server for your preferred IDE or AI client:

```bash
rememhq-cli init cursor --project my-project
```

## License

Apache License 2.0. See the [LICENSE](../LICENSE) file.
