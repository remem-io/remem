/**
 * remem TypeScript SDK — reasoning memory layer for AI agents.
 *
 * @example
 * ```ts
 * import { Memory } from "@remem/sdk";
 *
 * const m = new Memory({ project: "my-agent", reasoningModel: "gpt-4o" });
 * await m.store("User prefers TypeScript over JavaScript", { tags: ["prefs"] });
 * const results = await m.recall("language preferences");
 * ```
 */

import type {
  ConsolidationReport,
  CompactResponse,
  ForgetMode,
  MemoryConfig,
  MemoryResult,
  RecallOptions,
  SearchOptions,
  StoreOptions,
  StoreResponse,
  UpdateOptions,
} from "./types.js";

export type {
  ConsolidationReport,
  CompactResponse,
  ForgetMode,
  MemoryConfig,
  MemoryResult,
  RecallOptions,
  SearchOptions,
  StoreOptions,
  StoreResponse,
  UpdateOptions,
} from "./types.js";

export class Memory {
  private baseUrl: string;
  private headers: Record<string, string>;
  private timeout: number;

  constructor(config: MemoryConfig) {
    this.baseUrl = config.baseUrl ?? process.env.REMEM_BASE_URL ?? "http://localhost:7474";
    this.timeout = config.timeout ?? 30000;

    this.headers = { "Content-Type": "application/json" };
    const apiKey = config.apiKey ?? process.env.REMEM_API_KEY;
    if (apiKey) {
      this.headers["Authorization"] = `Bearer ${apiKey}`;
    }
  }

  /**
   * Store a new memory. The LLM scores importance automatically if not provided.
   */
  async store(content: string, options: StoreOptions = {}): Promise<StoreResponse> {
    const body = {
      content,
      tags: options.tags ?? [],
      importance: options.importance,
      ttl_days: options.ttl_days,
      memory_type: options.type ?? "fact",
    };

    const resp = await this.request("POST", "/v1/memories", body);
    return resp as StoreResponse;
  }

  /**
   * Guided recall — LLM re-ranks candidates for relevance.
   */
  async recall(query: string, options: RecallOptions = {}): Promise<MemoryResult[]> {
    const params = new URLSearchParams({ q: query });
    if (options.limit) params.set("limit", String(options.limit));
    if (options.filter_tags?.length) params.set("filter_tags", options.filter_tags.join(","));
    if (options.since) params.set("since", options.since);
    if (options.memory_type) params.set("memory_type", options.memory_type);

    return this.request("GET", `/v1/memories/recall?${params}`) as Promise<MemoryResult[]>;
  }

  /**
   * Hybrid vector + keyword search without LLM re-ranking.
   */
  async search(query: string, options: SearchOptions = {}): Promise<MemoryResult[]> {
    const params = new URLSearchParams({ q: query });
    if (options.limit) params.set("limit", String(options.limit));
    if (options.filter_tags?.length) params.set("filter_tags", options.filter_tags.join(","));

    return this.request("GET", `/v1/memories/search?${params}`) as Promise<MemoryResult[]>;
  }

  /**
   * Update an existing memory's content, importance, or tags.
   */
  async update(id: string, options: UpdateOptions): Promise<Record<string, unknown>> {
    return this.request("PATCH", `/v1/memories/${id}`, options) as Promise<Record<string, unknown>>;
  }

  /**
   * Delete, decay, or archive a memory.
   */
  async forget(id: string, mode: ForgetMode = "delete"): Promise<{ success: boolean }> {
    return this.request("DELETE", `/v1/memories/${id}?mode=${mode}`) as Promise<{
      success: boolean;
    }>;
  }

  /**
   * Trigger consolidation over a session's working memory.
   */
  async consolidate(sessionId: string, model?: string): Promise<ConsolidationReport> {
    const body = model ? { model } : {};
    return this.request(
      "POST",
      `/v1/sessions/${sessionId}/consolidate`,
      body
    ) as Promise<ConsolidationReport>;
  }

  /**
   * Apply importance-weighted decay to all active memories.
   */
  async decay(factor: number = 0.9): Promise<{ success: boolean; archived_count: number }> {
    return this.request("POST", "/v1/memories/decay", { factor }) as Promise<{
      success: boolean;
      archived_count: number;
    }>;
  }

  /**
   * Compact a conversation trace to save context window tokens.
   */
  async compactContext(
    conversationText: string,
    focusAreas?: string[]
  ): Promise<CompactResponse> {
    const body: Record<string, unknown> = { conversation_text: conversationText };
    if (focusAreas) {
      body.focus_areas = focusAreas;
    }
    return this.request("POST", "/v1/memories/compact", body) as Promise<CompactResponse>;
  }

  private async request(method: string, path: string, body?: unknown): Promise<unknown> {
    const url = `${this.baseUrl}${path}`;
    const init: RequestInit = {
      method,
      headers: this.headers,
      signal: AbortSignal.timeout(this.timeout),
    };

    if (body && (method === "POST" || method === "PATCH" || method === "PUT")) {
      init.body = JSON.stringify(body);
    }

    const resp = await fetch(url, init);

    if (!resp.ok) {
      const text = await resp.text().catch(() => "");
      throw new Error(`remem API error (${resp.status}): ${text}`);
    }

    return resp.json();
  }
}
