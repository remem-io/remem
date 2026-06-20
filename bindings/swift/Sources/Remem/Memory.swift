import Foundation

/// On-device reasoning memory layer for AI agents.
///
/// `Memory` wraps remem's native engine (via `rememhq-core`'s C ABI) so
/// iOS and macOS apps can store, recall, and reason over agent memory
/// entirely on-device — no server required. It mirrors the `Memory` API
/// shape used by remem's Python, TypeScript, and Rust SDKs.
///
/// ```swift
/// let memory = try await Memory.open(project: "my-agent")
/// let record = try await memory.store("User prefers dark mode", tags: ["preferences"])
/// let results = try await memory.recall("what are the user's preferences?")
/// ```
///
/// `Memory` is an `actor`: every call into the underlying engine is
/// serialized through it, so it's safe to share a single instance across
/// concurrent tasks.
public actor Memory {
    private let handle: EngineHandle

    private init(handle: EngineHandle) {
        self.handle = handle
    }

    /// Open (or create) a memory store for `project`.
    ///
    /// - Parameters:
    ///   - project: A name scoping this memory store. Separate projects
    ///     never share memories.
    ///   - dataDir: Directory to look for `.remem/config.toml` in, and
    ///     where the underlying SQLite database and vector index are
    ///     stored. Defaults to the engine's standard config search path
    ///     (typically `~/.remem`) when `nil`.
    public static func open(project: String = "default", dataDir: String? = nil) throws -> Memory {
        let handle = try EngineHandle(project: project, dataDir: dataDir)
        return Memory(handle: handle)
    }

    // MARK: - Memory operations

    /// Store a new memory.
    ///
    /// - Parameters:
    ///   - content: The text content to remember.
    ///   - tags: Classification tags for filtering later. Defaults to none.
    ///   - importance: A score from 1–10. Pass `nil` (the default) to let
    ///     the configured reasoning model score importance automatically.
    public func store(
        _ content: String,
        tags: [String] = [],
        importance: Float? = nil
    ) async throws -> MemoryRecord {
        let tagsJSON = try Self.encodeTags(tags)
        let data = try handle.store(
            content: content, tagsJSON: tagsJSON, importance: importance ?? -1
        )
        return try Self.decode(MemoryRecord.self, from: data)
    }

    /// Guided recall: vector + keyword search, re-ranked by the reasoning
    /// model for relevance, with each result annotated with why it
    /// matched.
    ///
    /// - Parameters:
    ///   - query: What to recall.
    ///   - limit: Maximum number of results. Defaults to 8.
    ///   - filterTags: Only consider memories with at least one matching
    ///     tag. Defaults to no filter.
    public func recall(
        _ query: String,
        limit: Int = 8,
        filterTags: [String] = []
    ) async throws -> [MemoryResult] {
        let tagsJSON = try Self.encodeTags(filterTags)
        let data = try handle.recall(query: query, limit: limit, filterTagsJSON: tagsJSON)
        return try Self.decode([MemoryResult].self, from: data)
    }

    /// Hybrid vector + keyword search without LLM re-ranking — faster
    /// than `recall`, with no reasoning-model call.
    public func search(
        _ query: String,
        limit: Int = 20,
        filterTags: [String] = []
    ) async throws -> [MemoryResult] {
        let tagsJSON = try Self.encodeTags(filterTags)
        let data = try handle.search(query: query, limit: limit, filterTagsJSON: tagsJSON)
        return try Self.decode([MemoryResult].self, from: data)
    }

    /// Update an existing memory. Any parameter left `nil` is unchanged.
    /// Pass an empty array for `tags` to clear all tags (this is different
    /// from leaving `tags` as `nil`, which leaves them untouched).
    public func update(
        id: UUID,
        content: String? = nil,
        importance: Float? = nil,
        tags: [String]? = nil
    ) async throws -> MemoryRecord {
        let tagsJSON = try tags.map { try Self.encodeTagsAllowingEmpty($0) }
        let data = try handle.update(
            id: id.uuidString, content: content, importance: importance, tagsJSON: tagsJSON
        )
        return try Self.decode(MemoryRecord.self, from: data)
    }

    /// Remove a memory.
    ///
    /// - Returns: `true` if a memory with this ID existed and was
    ///   removed, `false` if no such memory was found.
    @discardableResult
    public func forget(id: UUID, mode: ForgetMode = .delete) async throws -> Bool {
        try handle.forget(id: id.uuidString, mode: mode)
    }

    /// Apply importance-weighted decay across all active memories.
    ///
    /// - Returns: The number of memories archived as a result.
    @discardableResult
    public func decay(factor: Float = 0.9) async throws -> Int {
        try handle.decay(factor: factor)
    }

    // MARK: - Knowledge graph

    /// Query the knowledge graph by subject/predicate/object triple. Any
    /// parameter left `nil` matches anything in that position.
    public func queryKnowledge(
        subject: String? = nil,
        predicate: String? = nil,
        object: String? = nil
    ) async throws -> [KnowledgeGraphTriple] {
        let data = try handle.queryKnowledge(subject: subject, predicate: predicate, object: object)
        return try Self.decode([KnowledgeGraphTriple].self, from: data)
    }

    /// Fetch every knowledge-graph triple touching `entity`, in either
    /// subject or object position.
    public func entityContext(_ entity: String) async throws -> [KnowledgeGraphTriple] {
        let data = try handle.entityContext(entity: entity)
        return try Self.decode([KnowledgeGraphTriple].self, from: data)
    }

    // MARK: - Sessions

    /// Start a new tracked session with the given caller-supplied ID
    /// (e.g. a conversation ID). Memories don't need a session to be
    /// stored — sessions are an optional way to group related activity
    /// together for later consolidation.
    public func startSession(id: String) async throws {
        try handle.createSession(id: id)
    }

    /// End a tracked session, stamping its end time. Does not trigger
    /// consolidation — call `consolidate(sessionId:)` separately if you
    /// want durable facts extracted from it.
    ///
    /// - Returns: `true` if a session with this ID existed and was ended,
    ///   `false` if no such session was found.
    @discardableResult
    public func endSession(id: String) async throws -> Bool {
        try handle.endSession(id: id)
    }

    /// Fetch a single session by ID, or `nil` if no session with that ID
    /// exists.
    public func getSession(id: String) async throws -> Session? {
        guard let data = try handle.getSession(id: id) else { return nil }
        return try Self.decode(Session.self, from: data)
    }

    /// List the most recently started sessions.
    public func listSessions(limit: Int = 20) async throws -> [Session] {
        let data = try handle.listSessions(limit: limit)
        return try Self.decode([Session].self, from: data)
    }

    /// Run consolidation over a session's accumulated activity, extracting
    /// durable facts, detecting contradictions with existing memories, and
    /// updating the knowledge graph.
    ///
    /// - Parameter model: Reasoning model to use for consolidation.
    ///   Defaults to the engine's configured reasoning model when `nil`.
    public func consolidate(sessionId: String, model: String? = nil) async throws
        -> ConsolidationReport
    {
        let data = try handle.consolidate(sessionId: sessionId, model: model)
        return try Self.decode(ConsolidationReport.self, from: data)
    }

    // MARK: - Encoding/decoding helpers

    private static func encodeTags(_ tags: [String]) throws -> String? {
        guard !tags.isEmpty else { return nil }
        return try encodeTagsAllowingEmpty(tags)
    }

    /// Like `encodeTags`, but always produces a JSON array — including
    /// `"[]"` for an empty input — rather than collapsing empty to `nil`.
    /// Used by `update`, where a non-nil empty array means "clear tags"
    /// and is meaningfully different from "leave tags unchanged" (`nil`).
    private static func encodeTagsAllowingEmpty(_ tags: [String]) throws -> String {
        guard let data = try? JSONEncoder().encode(tags),
            let json = String(data: data, encoding: .utf8)
        else {
            throw RememError.encodingFailed("Could not encode tags as JSON: \(tags)")
        }
        return json
    }

    private static func decode<T: Decodable>(_ type: T.Type, from data: Data) throws -> T {
        do {
            return try RememCoding.decoder.decode(type, from: data)
        } catch {
            throw RememError.decodingFailed(
                "\(error) — raw payload: \(String(data: data, encoding: .utf8) ?? "<non-utf8>")"
            )
        }
    }
}
