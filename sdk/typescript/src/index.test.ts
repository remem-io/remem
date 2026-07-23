/**
 * Comprehensive tests for the remem TypeScript SDK.
 * Uses Node.js built-in test runner + mock.method to intercept fetch.
 */

import { describe, it, beforeEach, afterEach, mock } from "node:test";
import assert from "node:assert/strict";
import { Memory } from "./index.js";

const BASE = "http://localhost:7474";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function memoryResult(content = "test content", overrides: Record<string, unknown> = {}) {
  return {
    id: crypto.randomUUID(),
    content,
    importance: 5.0,
    tags: [],
    memory_type: "fact",
    created_at: new Date().toISOString(),
    source_session: null,
    similarity: 0.85,
    decay_score: 1.0,
    reasoning: null,
    ...overrides,
  };
}

function storeResponse(overrides: Record<string, unknown> = {}) {
  return {
    id: crypto.randomUUID(),
    importance: 7.0,
    tags: [],
    created_at: new Date().toISOString(),
    ...overrides,
  };
}

function consolidationReport(overrides: Record<string, unknown> = {}) {
  return {
    session_id: "sess-1",
    new_facts: 2,
    updated_facts: 0,
    contradictions: [],
    knowledge_graph_updates: [],
    ...overrides,
  };
}

function mockFetch(responseBody: unknown, status = 200) {
  return mock.method(globalThis, "fetch", async () => ({
    ok: status >= 200 && status < 300,
    status,
    json: async () => responseBody,
    text: async () => JSON.stringify(responseBody),
  }));
}

// ---------------------------------------------------------------------------
// store()
// ---------------------------------------------------------------------------

describe("Memory.store()", () => {
  afterEach(() => mock.restoreAll());

  it("returns a StoreResponse on success", async () => {
    mockFetch(storeResponse({ importance: 8.5 }), 201);
    const m = new Memory({ project: "test", baseUrl: BASE });
    const r = await m.store("hello");
    assert.equal(r.importance, 8.5);
    assert.ok(r.id);
  });

  it("storeBatch stores multiple items", async () => {
    mockFetch(storeResponse({ importance: 8.0 }), 200);
    const m = new Memory({ project: "test", baseUrl: BASE });
    const results = await m.storeBatch([
      { content: "First" },
      { content: "Second" },
    ]);
    assert.equal(results.length, 2);
    assert.equal(results[0].importance, 8.0);
  });

  it("sends content, tags, and importance in POST body", async () => {
    let body: Record<string, unknown> = {};
    mock.method(globalThis, "fetch", async (_url: string, init: RequestInit) => {
      body = JSON.parse(init.body as string);
      return { ok: true, status: 201, json: async () => storeResponse() };
    });
    const m = new Memory({ project: "test", baseUrl: BASE });
    await m.store("tagged", { tags: ["x", "y"], importance: 9.0 });
    assert.deepEqual(body.tags, ["x", "y"]);
    assert.equal(body.importance, 9.0);
    assert.equal(body.content, "tagged");
  });

  it("sends ttl_days and type fields", async () => {
    let body: Record<string, unknown> = {};
    mock.method(globalThis, "fetch", async (_url: string, init: RequestInit) => {
      body = JSON.parse(init.body as string);
      return { ok: true, status: 201, json: async () => storeResponse() };
    });
    const m = new Memory({ project: "test", baseUrl: BASE });
    await m.store("proc", { ttl_days: 30, type: "procedure" });
    assert.equal(body.ttl_days, 30);
    assert.equal(body.memory_type, "procedure");
  });

  it("posts to /v1/memories", async () => {
    let capturedUrl = "";
    mock.method(globalThis, "fetch", async (url: string) => {
      capturedUrl = url;
      return { ok: true, status: 201, json: async () => storeResponse() };
    });
    const m = new Memory({ project: "test", baseUrl: BASE });
    await m.store("test");
    assert.equal(capturedUrl, `${BASE}/v1/memories`);
  });

  it("throws on non-2xx response", async () => {
    mockFetch({ error: "server error" }, 500);
    const m = new Memory({ project: "test", baseUrl: BASE });
    await assert.rejects(() => m.store("bad"), /500/);
  });
});

// ---------------------------------------------------------------------------
// recall()
// ---------------------------------------------------------------------------

describe("Memory.recall()", () => {
  afterEach(() => mock.restoreAll());

  it("returns an array of MemoryResults", async () => {
    mockFetch([memoryResult("A"), memoryResult("B")]);
    const m = new Memory({ project: "test", baseUrl: BASE });
    const results = await m.recall("query");
    assert.equal(results.length, 2);
    assert.equal(results[0].content, "A");
  });

  it("encodes q, limit, and filter_tags as query params", async () => {
    let capturedUrl = "";
    mock.method(globalThis, "fetch", async (url: string) => {
      capturedUrl = url;
      return { ok: true, status: 200, json: async () => [] };
    });
    const m = new Memory({ project: "test", baseUrl: BASE });
    await m.recall("my query", { limit: 5, filter_tags: ["tag1", "tag2"] });
    assert.ok(capturedUrl.includes("q=my+query") || capturedUrl.includes("q=my%20query"));
    assert.ok(capturedUrl.includes("limit=5"));
    assert.ok(capturedUrl.includes("filter_tags=tag1%2Ctag2") || capturedUrl.includes("filter_tags=tag1,tag2"));
  });

  it("encodes memory_type filter", async () => {
    let capturedUrl = "";
    mock.method(globalThis, "fetch", async (url: string) => {
      capturedUrl = url;
      return { ok: true, status: 200, json: async () => [] };
    });
    const m = new Memory({ project: "test", baseUrl: BASE });
    await m.recall("q", { memory_type: "procedure" });
    assert.ok(capturedUrl.includes("memory_type=procedure"));
  });

  it("returns empty array when no results", async () => {
    mockFetch([]);
    const m = new Memory({ project: "test", baseUrl: BASE });
    const results = await m.recall("nothing");
    assert.deepEqual(results, []);
  });
});

// ---------------------------------------------------------------------------
// search()
// ---------------------------------------------------------------------------

describe("Memory.search()", () => {
  afterEach(() => mock.restoreAll());

  it("returns results array", async () => {
    mockFetch([memoryResult()]);
    const m = new Memory({ project: "test", baseUrl: BASE });
    const results = await m.search("deploy");
    assert.equal(results.length, 1);
  });

  it("sends correct limit param", async () => {
    let capturedUrl = "";
    mock.method(globalThis, "fetch", async (url: string) => {
      capturedUrl = url;
      return { ok: true, status: 200, json: async () => [] };
    });
    const m = new Memory({ project: "test", baseUrl: BASE });
    await m.search("q", { limit: 15 });
    assert.ok(capturedUrl.includes("limit=15"));
  });

  it("hits /v1/memories/search", async () => {
    let capturedUrl = "";
    mock.method(globalThis, "fetch", async (url: string) => {
      capturedUrl = url;
      return { ok: true, status: 200, json: async () => [] };
    });
    const m = new Memory({ project: "test", baseUrl: BASE });
    await m.search("test");
    assert.ok(capturedUrl.includes("/v1/memories/search"));
  });
});

// ---------------------------------------------------------------------------
// update()
// ---------------------------------------------------------------------------

describe("Memory.update()", () => {
  afterEach(() => mock.restoreAll());

  it("sends PATCH to /v1/memories/:id", async () => {
    let capturedMethod = "";
    let capturedUrl = "";
    mock.method(globalThis, "fetch", async (url: string, init: RequestInit) => {
      capturedMethod = init.method ?? "";
      capturedUrl = url;
      return { ok: true, status: 200, json: async () => ({ id: "abc", content: "new", importance: 8.0, tags: [], updated_at: "" }) };
    });
    const m = new Memory({ project: "test", baseUrl: BASE });
    await m.update("abc", { content: "new", importance: 8.0 });
    assert.equal(capturedMethod, "PATCH");
    assert.ok(capturedUrl.includes("/v1/memories/abc"));
  });

  it("returns the updated record", async () => {
    mockFetch({ id: "x", content: "updated", importance: 9.0, tags: ["new"], updated_at: "now" });
    const m = new Memory({ project: "test", baseUrl: BASE });
    const r = await m.update("x", { content: "updated" }) as Record<string, unknown>;
    assert.equal(r.content, "updated");
    assert.equal(r.importance, 9.0);
  });
});

// ---------------------------------------------------------------------------
// forget()
// ---------------------------------------------------------------------------

describe("Memory.forget()", () => {
  afterEach(() => mock.restoreAll());

  it("sends DELETE with mode=delete by default", async () => {
    let capturedMethod = "";
    let capturedUrl = "";
    mock.method(globalThis, "fetch", async (url: string, init: RequestInit) => {
      capturedMethod = init.method ?? "";
      capturedUrl = url;
      return { ok: true, status: 200, json: async () => ({ success: true }) };
    });
    const m = new Memory({ project: "test", baseUrl: BASE });
    await m.forget("abc");
    assert.equal(capturedMethod, "DELETE");
    assert.ok(capturedUrl.includes("mode=delete"));
  });

  it("sends mode=archive when requested", async () => {
    let capturedUrl = "";
    mock.method(globalThis, "fetch", async (url: string) => {
      capturedUrl = url;
      return { ok: true, status: 200, json: async () => ({ success: true }) };
    });
    const m = new Memory({ project: "test", baseUrl: BASE });
    await m.forget("abc", "archive");
    assert.ok(capturedUrl.includes("mode=archive"));
  });

  it("returns success flag", async () => {
    mockFetch({ success: true });
    const m = new Memory({ project: "test", baseUrl: BASE });
    const r = await m.forget("x");
    assert.equal(r.success, true);
  });
});

// ---------------------------------------------------------------------------
// consolidate()
// ---------------------------------------------------------------------------

describe("Memory.consolidate()", () => {
  afterEach(() => mock.restoreAll());

  it("posts to /v1/sessions/:id/consolidate", async () => {
    let capturedUrl = "";
    mock.method(globalThis, "fetch", async (url: string) => {
      capturedUrl = url;
      return { ok: true, status: 200, json: async () => consolidationReport() };
    });
    const m = new Memory({ project: "test", baseUrl: BASE });
    await m.consolidate("sess-42");
    assert.ok(capturedUrl.includes("/v1/sessions/sess-42/consolidate"));
  });

  it("returns a ConsolidationReport", async () => {
    mockFetch(consolidationReport({ new_facts: 5, updated_facts: 2 }));
    const m = new Memory({ project: "test", baseUrl: BASE });
    const r = await m.consolidate("s");
    assert.equal(r.new_facts, 5);
    assert.equal(r.updated_facts, 2);
    assert.ok(Array.isArray(r.contradictions));
  });

  it("sends model override in body", async () => {
    let body: Record<string, unknown> = {};
    mock.method(globalThis, "fetch", async (_url: string, init: RequestInit) => {
      body = JSON.parse(init.body as string);
      return { ok: true, status: 200, json: async () => consolidationReport() };
    });
    const m = new Memory({ project: "test", baseUrl: BASE });
    await m.consolidate("s1", "gemini-2.0-flash");
    assert.equal(body.model, "gemini-2.0-flash");
  });
});

// ---------------------------------------------------------------------------
// decay()
// ---------------------------------------------------------------------------

describe("Memory.decay()", () => {
  afterEach(() => mock.restoreAll());

  it("sends POST with factor", async () => {
    let body: Record<string, unknown> = {};
    mock.method(globalThis, "fetch", async (_url: string, init: RequestInit) => {
      body = JSON.parse(init.body as string);
      return { ok: true, status: 200, json: async () => ({ success: true, archived_count: 3 }) };
    });
    const m = new Memory({ project: "test", baseUrl: BASE });
    const r = await m.decay(0.75);
    assert.equal(body.factor, 0.75);
    assert.equal(r.archived_count, 3);
  });

  it("defaults factor to 0.9", async () => {
    let body: Record<string, unknown> = {};
    mock.method(globalThis, "fetch", async (_url: string, init: RequestInit) => {
      body = JSON.parse(init.body as string);
      return { ok: true, status: 200, json: async () => ({ success: true, archived_count: 0 }) };
    });
    const m = new Memory({ project: "test", baseUrl: BASE });
    await m.decay();
    assert.equal(body.factor, 0.9);
  });
});

// ---------------------------------------------------------------------------
// Auth header
// ---------------------------------------------------------------------------

describe("Authentication", () => {
  afterEach(() => mock.restoreAll());

  it("sends Authorization: Bearer header when apiKey is set", async () => {
    let authHeader = "";
    mock.method(globalThis, "fetch", async (_url: string, init: RequestInit) => {
      authHeader = (init.headers as Record<string, string>)["Authorization"] ?? "";
      return { ok: true, status: 201, json: async () => storeResponse() };
    });
    const m = new Memory({ project: "test", baseUrl: BASE, apiKey: "my-secret" });
    await m.store("test");
    assert.equal(authHeader, "Bearer my-secret");
  });

  it("omits Authorization header when no apiKey", async () => {
    let headers: Record<string, string> = {};
    mock.method(globalThis, "fetch", async (_url: string, init: RequestInit) => {
      headers = init.headers as Record<string, string>;
      return { ok: true, status: 201, json: async () => storeResponse() };
    });
    const m = new Memory({ project: "test", baseUrl: BASE });
    await m.store("test");
    assert.equal(headers["Authorization"], undefined);
  });
});
