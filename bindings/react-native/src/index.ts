import type {
  ConsolidationReport,
  ForgetMode,
  KnowledgeGraphTriple,
  MemoryRecord,
  MemoryResult,
  Session,
} from './Remem.types';
import NativeRemem from './RememModule';

export * from './Remem.types';

function encodeTags(tags: string[] | undefined): string | null {
  if (!tags || tags.length === 0) return null;
  return JSON.stringify(tags);
}

/**
 * Like {@link encodeTags}, but always produces a JSON array — including
 * `"[]"` for an empty input — rather than collapsing empty to `null`.
 * Used by {@link Memory.update}, where a non-null empty array means
 * "clear tags" and is meaningfully different from "leave tags unchanged"
 * (`undefined`/omitted).
 */
function encodeTagsAllowingEmpty(tags: string[]): string {
  return JSON.stringify(tags);
}

/**
 * On-device reasoning memory layer for AI agents.
 *
 * `Memory` wraps remem's native engine (via rememhq-core's C ABI, through
 * an Expo Modules native binding) so React Native apps can store, recall,
 * and reason over agent memory entirely on-device — no server required.
 * It mirrors the `Memory` API shape used by remem's Python, Swift, and
 * Rust SDKs.
 *
 * ```ts
 * const memory = await Memory.open({ project: 'my-agent' });
 * const record = await memory.store('User prefers dark mode', { tags: ['preferences'] });
 * const results = await memory.recall("what are the user's preferences?");
 * await memory.close();
 * ```
 *
 * Each `Memory` instance owns one native engine handle. Calling
 * {@link Memory.close} releases it — failing to call `close()` leaks the
 * native engine (and its open SQLite connection) for the lifetime of the
 * app, so call it when you're done with an instance (e.g. in a
 * `useEffect` cleanup function).
 */
export class Memory {
  private constructor(private readonly engineId: number) {}

  /**
   * Open (or create) a memory store for `project`.
   *
   * @param project A name scoping this memory store. Separate projects
   *   never share memories. Defaults to `"default"`.
   * @param dataDir Directory to look for `.remem/config.toml` in, and
   *   where the underlying SQLite database and vector index are stored.
   *   Defaults to the engine's standard config search path when omitted.
   *   On iOS, app sandboxing means you'll typically want this set to
   *   something under your app's documents/library directory rather
   *   than relying on the default.
   */
  static async open(options: { project?: string; dataDir?: string } = {}): Promise<Memory> {
    const engineId = await NativeRemem.openEngine(
      options.project ?? 'default',
      options.dataDir ?? null
    );
    return new Memory(engineId);
  }

  /** Release the native engine handle. Safe to call more than once. */
  async close(): Promise<void> {
    await NativeRemem.closeEngine(this.engineId);
  }

  // ---------------------------------------------------------------------
  // Memory operations
  // ---------------------------------------------------------------------

  /**
   * Store a new memory.
   *
   * @param content The text content to remember.
   * @param options.tags Classification tags for filtering later. Defaults to none.
   * @param options.importance A score from 1–10. Omit to let the
   *   configured reasoning model score importance automatically.
   */
  async store(
    content: string,
    options: { tags?: string[]; importance?: number } = {}
  ): Promise<MemoryRecord> {
    const result = await NativeRemem.store(
      this.engineId,
      content,
      encodeTags(options.tags),
      options.importance ?? -1
    );
    return result as unknown as MemoryRecord;
  }

  /**
   * Guided recall: vector + keyword search, re-ranked by the reasoning
   * model for relevance, with each result annotated with why it matched.
   *
   * @param query What to recall.
   * @param options.limit Maximum number of results. Defaults to 8.
   * @param options.filterTags Only consider memories with at least one
   *   matching tag. Defaults to no filter.
   */
  async recall(
    query: string,
    options: { limit?: number; filterTags?: string[] } = {}
  ): Promise<MemoryResult[]> {
    const results = await NativeRemem.recall(
      this.engineId,
      query,
      options.limit ?? 8,
      encodeTags(options.filterTags)
    );
    return results as unknown as MemoryResult[];
  }

  /**
   * Hybrid vector + keyword search without LLM re-ranking — faster than
   * {@link recall}, with no reasoning-model call.
   */
  async search(
    query: string,
    options: { limit?: number; filterTags?: string[] } = {}
  ): Promise<MemoryResult[]> {
    const results = await NativeRemem.search(
      this.engineId,
      query,
      options.limit ?? 20,
      encodeTags(options.filterTags)
    );
    return results as unknown as MemoryResult[];
  }

  /**
   * Update an existing memory. Any option left unset is unchanged.
   * Pass an empty array for `tags` to clear all tags (this is different
   * from leaving `tags` unset, which leaves them untouched).
   */
  async update(
    id: string,
    options: { content?: string; importance?: number; tags?: string[] } = {}
  ): Promise<MemoryRecord> {
    const tagsJSON = options.tags !== undefined ? encodeTagsAllowingEmpty(options.tags) : null;
    const result = await NativeRemem.update(
      this.engineId,
      id,
      options.content ?? null,
      options.importance ?? -1,
      tagsJSON
    );
    return result as unknown as MemoryRecord;
  }

  /**
   * Remove a memory.
   *
   * @returns `true` if a memory with this ID existed and was removed,
   *   `false` if no such memory was found.
   */
  async forget(id: string, mode: ForgetMode = 'delete'): Promise<boolean> {
    return NativeRemem.forget(this.engineId, id, mode);
  }

  /**
   * Apply importance-weighted decay across all active memories.
   *
   * @returns The number of memories archived as a result.
   */
  async decay(factor = 0.9): Promise<number> {
    return NativeRemem.decay(this.engineId, factor);
  }

  // ---------------------------------------------------------------------
  // Knowledge graph
  // ---------------------------------------------------------------------

  /**
   * Query the knowledge graph by subject/predicate/object triple. Any
   * option left unset matches anything in that position.
   */
  async queryKnowledge(
    options: { subject?: string; predicate?: string; object?: string } = {}
  ): Promise<KnowledgeGraphTriple[]> {
    const results = await NativeRemem.queryKnowledge(
      this.engineId,
      options.subject ?? null,
      options.predicate ?? null,
      options.object ?? null
    );
    return results as unknown as KnowledgeGraphTriple[];
  }

  /**
   * Fetch every knowledge-graph triple touching `entity`, in either
   * subject or object position.
   */
  async entityContext(entity: string): Promise<KnowledgeGraphTriple[]> {
    const results = await NativeRemem.entityContext(this.engineId, entity);
    return results as unknown as KnowledgeGraphTriple[];
  }

  // ---------------------------------------------------------------------
  // Sessions
  // ---------------------------------------------------------------------

  /**
   * Start a new tracked session with the given caller-supplied ID (e.g. a
   * conversation ID). Memories don't need a session to be stored —
   * sessions are an optional way to group related activity together for
   * later consolidation.
   */
  async startSession(id: string): Promise<void> {
    await NativeRemem.startSession(this.engineId, id);
  }

  /**
   * End a tracked session, stamping its end time. Does not trigger
   * consolidation — call {@link consolidate} separately if you want
   * durable facts extracted from it.
   *
   * @returns `true` if a session with this ID existed and was ended,
   *   `false` if no such session was found.
   */
  async endSession(id: string): Promise<boolean> {
    return NativeRemem.endSession(this.engineId, id);
  }

  /** Fetch a single session by ID, or `null` if no session with that ID exists. */
  async getSession(id: string): Promise<Session | null> {
    const result = await NativeRemem.getSession(this.engineId, id);
    return result as unknown as Session | null;
  }

  /** List the most recently started sessions. */
  async listSessions(limit = 20): Promise<Session[]> {
    const results = await NativeRemem.listSessions(this.engineId, limit);
    return results as unknown as Session[];
  }

  /**
   * Run consolidation over a session's accumulated activity, extracting
   * durable facts, detecting contradictions with existing memories, and
   * updating the knowledge graph.
   *
   * @param model Reasoning model to use for consolidation. Defaults to
   *   the engine's configured reasoning model when omitted.
   */
  async consolidate(sessionId: string, model?: string): Promise<ConsolidationReport> {
    const result = await NativeRemem.consolidate(this.engineId, sessionId, model ?? null);
    return result as unknown as ConsolidationReport;
  }
}
