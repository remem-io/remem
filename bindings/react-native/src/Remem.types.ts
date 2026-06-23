/**
 * Types mirroring the JSON shapes returned by rememhq-core's engine FFI,
 * as documented in rememhq-core/include/rememhq.h. These match
 * bindings/swift's MemoryModels.swift field-for-field, except dates stay
 * as ISO 8601 strings here rather than being parsed into Date objects —
 * JS callers can do `new Date(record.createdAt)` themselves if they want
 * Date objects; leaving the choice to them avoids baking in a conversion
 * that not everyone wants on a hot path like `recall`.
 */

/** The four memory types in remem's taxonomy. */
export type MemoryType = 'fact' | 'procedure' | 'preference' | 'decision';

/** How a memory should be removed via {@link Memory.forget}. */
export type ForgetMode = 'delete' | 'decay' | 'archive';

/** A stored memory, as returned by {@link Memory.store} and {@link Memory.update}. */
export interface MemoryRecord {
  id: string;
  content: string;
  importance: number;
  tags: string[];
  memoryType: MemoryType;
  createdAt: string;
  updatedAt: string;
  decayScore: number;
  sourceSession: string | null;
  ttlDays: number | null;
}

/** A memory returned from {@link Memory.recall} or {@link Memory.search}. */
export interface MemoryResult {
  id: string;
  content: string;
  importance: number;
  tags: string[];
  memoryType: MemoryType;
  createdAt: string;
  sourceSession: string | null;
  /** Vector similarity score (0.0–1.0). */
  similarity: number;
  decayScore: number;
  /**
   * Present only for `recall` (LLM-guided), explaining why this result
   * was judged relevant. Always `null` for `search`.
   */
  reasoning: string | null;
}

/** A single subject–predicate–object fact in the knowledge graph. */
export interface KnowledgeGraphTriple {
  subject: string;
  predicate: string;
  object: string;
}

/**
 * A contradiction detected during consolidation between a newly observed
 * fact and an existing memory.
 */
export interface Contradiction {
  existingMemoryId: string;
  newContent: string;
  existingContent: string;
  explanation: string;
}

/** The result of running {@link Memory.consolidate} over a session. */
export interface ConsolidationReport {
  sessionId: string;
  newFacts: number;
  updatedFacts: number;
  contradictions: Contradiction[];
  knowledgeGraphUpdates: KnowledgeGraphTriple[];
}

/**
 * A tracked session, as returned by {@link Memory.startSession} (indirectly),
 * {@link Memory.getSession}, and {@link Memory.listSessions}.
 */
export interface Session {
  id: string;
  project: string;
  startedAt: string;
  endedAt: string | null;
  consolidated: boolean;
  memoryCount: number;
}
