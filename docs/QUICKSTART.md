# Remem Quickstart

Welcome to remem! This guide will help you get started quickly depending on your setup.

## 1. Using with MCP Clients (Claude Code, Cursor, Copilot)

Remem operates as an MCP server.

### Configuration

Add the following to your MCP client configuration:

```json
{
  "mcpServers": {
    "remem": {
      "command": "rememhq",
      "args": ["mcp", "--project", "my-project"]
    }
  }
}
```

## 2. Using with Python

Install the SDK:
```bash
pip install rememhq
```

Basic script:
```python
import asyncio
from rememhq import Memory

async def main():
    async with Memory(project="test-project", base_url="http://127.0.0.1:7474") as memory:
        await memory.store("The codebase uses Python 3.11", tags=["infra"])
        results = await memory.recall("What Python version?")
        print(results[0].content)

asyncio.run(main())
```

## 3. Using with TypeScript

Install the SDK:
```bash
npm install @rememhq/sdk
```

Basic script:
```typescript
import { Memory } from "@rememhq/sdk";

async function main() {
    const memory = new Memory({ project: "test-project", baseUrl: "http://127.0.0.1:7474" });
    await memory.store("We use ESLint for linting", { tags: ["tooling"] });
    const results = await memory.recall("What linter is used?");
    console.log(results[0].content);
}

main();
```

## Next Steps
Check the `examples/` folder for more complex `multi_agent.py` and `rag_example.py` implementations!
