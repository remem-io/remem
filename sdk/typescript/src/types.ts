/**
 * Core type definitions for the remem TypeScript SDK.
 */

export type MemoryType = "fact" | "procedure" | "preference" | "decision";
export type ForgetMode = "delete" | "decay" | "archive";

export interface StoreOptions {
  tags?: string[];
  importance?: number;
  ttl_days?: number;
  type?: MemoryType;
}

export interface RecallOptions {
  limit?: number;
  filter_tags?: string[];
  since?: string; // ISO 8601
  memory_type?: MemoryType;
}

export interface SearchOptions {
  limit?: number;
  filter_tags?: string[];
}

export interface UpdateOptions {
  content?: string;
  importance?: number;
  tags?: string[];
}

export interface StoreResponse {
  id: string;
  importance: number;
  tags: string[];
  created_at: string;
}

export interface MemoryResult {
  id: string;
  content: string;
  importance: number;
  tags: string[];
  memory_type: MemoryType;
  created_at: string;
  source_session?: string;
  similarity: number;
  decay_score: number;
  reasoning?: string;
}

export interface ConsolidationReport {
  session_id: string;
  new_facts: number;
  updated_facts: number;
  contradictions: Contradiction[];
  knowledge_graph_updates: KnowledgeGraphUpdate[];
}

export interface CompactResponse {
  compressed_context: string;
  original_length: number;
  compressed_length: number;
}

export interface Contradiction {
  existing_memory_id: string;
  new_content: string;
  existing_content: string;
  explanation: string;
}

export interface KnowledgeGraphUpdate {
  subject: string;
  predicate: string;
  object: string;
}

export interface MemoryConfig {
  project: string;
  reasoningModel?: string;
  scoringModel?: string;
  baseUrl?: string;
  apiKey?: string;
  timeout?: number;
}

export interface MemoryStoreRecord {
  id: string;
  name: string;
  description?: string;
  created_at: string;
  updated_at: string;
  archived: boolean;
}

export interface MemoryVersionRecord {
  id: string;
  store_id: string;
  memory_id: string;
  operation: string;
  content: string;
  content_sha256: string;
  created_at: string;
}

export interface MetricsSnapshot {
  total_stores: number;
  total_recalls: number;
  total_consolidations: number;
  store_latency_p50_ms: number;
  store_latency_p95_ms: number;
  recall_latency_p50_ms: number;
  recall_latency_p95_ms: number;
  recall_latency_p99_ms: number;
  active_sessions: number;
  uptime_seconds: number;
}

export interface CostSummary {
  total_calls: number;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  estimated_cost_usd: number;
  usage_by_provider: Record<string, number>;
}

export interface CacheStats {
  hits: number;
  misses: number;
  total_entries: number;
  hit_rate_percentage: number;
}

export interface TelemetryResponse {
  metrics: MetricsSnapshot;
  cost_meter: CostSummary;
  cache_stats: CacheStats;
}
