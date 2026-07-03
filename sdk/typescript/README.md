# @rememhq/sdk

[![npm version](https://badge.fury.io/js/@rememhq%2Fsdk.svg)](https://badge.fury.io/js/@rememhq%2Fsdk)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![TypeScript](https://img.shields.io/badge/TypeScript-Ready-blue.svg)](https://www.typescriptlang.org/)

The official TypeScript SDK for **remem** — the reasoning memory layer for AI agents.

Remem provides a persistent, queryable memory system that uses LLM-powered reasoning for importance scoring, contradiction detection, knowledge graph construction, and session consolidation. This SDK allows seamless integration of remem into your TypeScript and Node.js applications.

## Key Features

- **Semantic Memory**: Store and recall memories using natural language.
- **LLM-Powered Reasoning**: Automatically scores memory importance and detects contradictions.
- **Knowledge Graph**: Extracts and queries relationships between entities.
- **TypeScript Native**: Fully typed API with extensive TSDoc comments.
- **Universal**: Runs in modern browsers and Node.js 18+ via native `fetch()`.

## Installation

Install the package via npm, yarn, or pnpm:

```bash
npm install @rememhq/sdk
# or
yarn add @rememhq/sdk
# or
pnpm add @rememhq/sdk
```

## Quick Start

### 1. Start the remem API Server

Before using the SDK, start the `rememhq-api` server from the remem repository root:

```bash
cargo run -p rememhq-api -- --project default
```

By default, the server listens on `http://localhost:7474`.

### 2. Initialize the Client

```typescript
import { Memory } from "@rememhq/sdk";

// Initialize the memory client
const memory = new Memory({
  baseUrl: "http://localhost:7474",
  project: "my-agent",
  reasoningModel: "gpt-4o", // Configure reasoning model (e.g., claude-sonnet-4-6, gpt-4o)
  headers: {
    // Optional: Include API keys if authentication is configured on the server
    "Authorization": `Bearer ${process.env.REMEM_API_KEY}`
  }
});
```

### 3. Store and Recall Memories

```typescript
// Store a new memory
const storeResponse = await memory.store("The user prefers using TypeScript over JavaScript for large codebases.", {
  tags: ["preferences", "languages"],
  importance: 0.8 // Optional: Overrides LLM auto-scoring
});

console.log(`Stored memory ID: ${storeResponse.id}`);

// Recall relevant memories based on context
const results = await memory.recall("What is the user's preferred language for big projects?", 5);

for (const result of results) {
  console.log(`Content: ${result.content}`);
  console.log(`Relevance Score: ${result.score}`);
  console.log(`Reasoning: ${result.reasoning}`);
}
```

## API Reference

### Configuration Options (`MemoryOptions`)

| Option | Type | Default | Description |
|---|---|---|---|
| `baseUrl` | `string` | `"http://localhost:7474"` | URL of the remem API server |
| `project` | `string` | `"default"` | Logical namespace for your agent's memory |
| `reasoningModel` | `string` | `"gpt-4o"` | LLM used for reasoning and scoring |
| `timeout` | `number` | `30000` | Request timeout in milliseconds |
| `headers` | `Record<string, string>` | `{}` | Additional HTTP headers |

### Core Methods

- `store(content: string, options?: StoreOptions): Promise<StoreResponse>`: Saves a new memory.
- `recall(query: string, limit?: number): Promise<RecallResult[]>`: Retrieves contextually relevant memories with LLM reasoning.
- `search(query: string, limit?: number): Promise<SearchResult[]>`: Performs a fast vector/full-text search.
- `update(memoryId: string, content: string): Promise<UpdateResponse>`: Modifies an existing memory.
- `forget(memoryId: string): Promise<ForgetResponse>`: Deletes a memory from the store.
- `consolidate(sessionId: string): Promise<ConsolidateResponse>`: Consolidates temporary session logs into long-term facts.

## Development

To build the SDK locally:

```bash
cd sdk/typescript
npm install
npm run build
npm run test
```

## Contributing

We welcome contributions! Please review our [Contributing Guide](../../CONTRIBUTING.md) for details on submitting pull requests, reporting issues, and suggesting enhancements.

## License

This project is licensed under the Apache License 2.0. See the [LICENSE](../../LICENSE) file for more details.
