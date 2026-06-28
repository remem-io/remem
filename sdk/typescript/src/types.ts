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
