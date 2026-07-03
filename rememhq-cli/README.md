# rememhq-cli

[![Crates.io](https://img.shields.io/crates/v/rememhq-cli.svg)](https://crates.io/crates/rememhq-cli)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

The command-line interface for **remem** — the reasoning memory layer for AI agents.

This crate provides the `rememhq` CLI tool, which serves as an administrative control plane for interacting with your local remem databases, monitoring system metrics, and effortlessly configuring MCP integrations with supported AI clients.

## Installation

Install the CLI utility via Cargo:

```bash
cargo install rememhq-cli
```

## Commands

### `init`

Automatically initializes the remem Model Context Protocol (MCP) server configuration for popular AI coding agents.

```bash
rememhq init cursor --project my-project
rememhq init claude-code --project default
rememhq init all --project shared-memory
```

Supported clients include: `claude-code`, `codex`, `cursor`, `copilot`, `antigravity-cli`, `opencode`, and `all`.

### `stats`

View metrics and statistics about your local memory store, including total memory count, vector index size, and entity relationships.

```bash
rememhq stats --project my-project
```

### `query` (Coming soon)

Directly execute raw searches and recall operations from the terminal without writing code.

## Configuration

The CLI respects standard remem environment variables (like `REMEM_API_KEY`, `REMEM_PROVIDER`, etc.) and default data directories (`~/.remem`). 

## License

Apache License 2.0. See the [LICENSE](../LICENSE) file.
