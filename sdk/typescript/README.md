# @rememhq/sdk

TypeScript SDK for remem — reasoning memory layer for AI agents.

## Installation

```bash
npm install @rememhq/sdk
```

## Usage

```typescript
import { Memory } from "@rememhq/sdk";

const m = new Memory({ project: "my-agent" });
await m.store("User prefers TypeScript", { tags: ["tech"] });
```
