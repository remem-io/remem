# @rememhq/sdk

TypeScript SDK for remem — reasoning memory layer for AI agents.

## Installation

```bash
npm install @rememhq/sdk
```

## Quick Start

```typescript
import { Memory } from "@rememhq/sdk";

const m = new Memory({
  baseUrl: "http://localhost:7474",
  project: "my-agent",
  reasoningModel: "gpt-4"
});

// Store a memory
await m.store("User prefers TypeScript", { tags: ["language"] });

// Recall memories with reasoning
const results = await m.recall("what language does the user prefer?");
for (const result of results) {
  console.log(`Content: ${result.content}`);
  console.log(`Reasoning: ${result.reasoning}`);
}
```

## Development

### Prerequisites

- **Node.js** 20+ (or 18+)
- **npm** 10+ (or use **pnpm**, **yarn**)
- **TypeScript** 5+

### Setup

```bash
cd sdk/typescript

# Install dependencies
npm install

# Build TypeScript
npm run build

# Watch mode (auto-rebuild on changes)
npm run watch
```

### Testing

```bash
# Run all tests
npm test

# Run in watch mode
npm run test:watch

# Run with coverage
npm run test:coverage

# Run specific test file
npm test -- memory.test.ts
```

### Code Quality

```bash
# Format code (Prettier)
npm run format

# Check formatting
npm run format:check

# Lint (ESLint)
npm run lint

# Type check
npm run type-check
```

### Building

```bash
# Build for distribution
npm run build

# Generate declaration files
npm run build:types

# Bundle for browser/Node
npm run bundle
```

### Publishing

```bash
# Prepare for publish (lint, test, build)
npm run prepublishOnly

# Publish to npm (requires npm auth)
npm publish
```

## Running the API Server

Before using the SDK, start the remem API server from the repository root:

```bash
cargo run -p rememhq-api -- --project default
```

The API will be available at `http://localhost:7474` by default.

## API Reference

### Memory

#### `store(content: string, options?: StoreOptions): Promise<StoreResponse>`

Store a memory with optional tags and importance score.

```typescript
const response = await m.store("Production DB is PostgreSQL 15", {
  tags: ["infra", "database"],
  importance: 0.95
});
```

#### `recall(query: string, limit?: number): Promise<RecallResult[]>`

Retrieve memories most relevant to the query with reasoning traces.

```typescript
const results = await m.recall("what database do we use?", 8);
results.forEach(r => {
  console.log(`Content: ${r.content}`);
  console.log(`Score: ${r.score}`);
  console.log(`Reasoning: ${r.reasoning}`);
});
```

#### `search(query: string, limit?: number): Promise<SearchResult[]>`

Full-text search over all memories.

```typescript
const results = await m.search("deployment", 10);
```

#### `update(memoryId: string, content: string): Promise<UpdateResponse>`

Update an existing memory's content.

#### `forget(memoryId: string): Promise<ForgetResponse>`

Delete a memory.

#### `consolidate(sessionId: string): Promise<ConsolidateResponse>`

Consolidate session logs into durable facts.

## Configuration

Configure the SDK via constructor options or environment variables:

```typescript
const m = new Memory({
  baseUrl: "http://localhost:7474",
  project: "my-agent",
  reasoningModel: "gpt-4",
  timeout: 30000,
  headers: {
    "Authorization": `Bearer ${process.env.API_KEY}`
  }
});
```

**Environment variables:**
- `REMEM_API_URL` — API server URL (default: `http://localhost:7474`)
- `REMEM_PROJECT` — Project name (default: `default`)
- `REMEM_REASONING_MODEL` — Reasoning model (default: `gpt-4`)
- `REMEM_TIMEOUT` — Request timeout in milliseconds (default: `30000`)

## Examples

See `examples/` directory for complete working examples.

## Browser Support

The SDK uses native `fetch()` and works in modern browsers and Node.js 18+.

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for guidelines on contributing to the TypeScript SDK.

## License

Apache License 2.0 — See [LICENSE](../../LICENSE) for details.
