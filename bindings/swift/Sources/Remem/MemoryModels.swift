import Foundation

/// The four memory types in remem's taxonomy.
public enum MemoryType: String, Codable, Sendable {
    case fact
    case procedure
    case preference
    case decision
}

/// How a memory should be removed via `Memory.forget`.
public enum ForgetMode: String, Sendable {
    case delete
    case decay
    case archive
}

/// A stored memory, as returned by `Memory.store` and `Memory.update`.
public struct MemoryRecord: Codable, Sendable, Identifiable {
    public let id: UUID
    public let content: String
    public let importance: Float
    public let tags: [String]
    public let memoryType: MemoryType
    public let createdAt: Date
    public let updatedAt: Date
    public let decayScore: Float
    public let sourceSession: String?
    public let ttlDays: UInt32?

    private enum CodingKeys: String, CodingKey {
        case id, content, importance, tags
        case memoryType = "memory_type"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case decayScore = "decay_score"
        case sourceSession = "source_session"
        case ttlDays = "ttl_days"
    }
}

/// A memory returned from `Memory.recall` or `Memory.search`, with
/// relevance metadata attached.
public struct MemoryResult: Codable, Sendable, Identifiable {
    public let id: UUID
    public let content: String
    public let importance: Float
    public let tags: [String]
    public let memoryType: MemoryType
    public let createdAt: Date
    public let sourceSession: String?
    /// Vector similarity score (0.0–1.0).
    public let similarity: Float
    public let decayScore: Float
    /// Present only for `recall` (LLM-guided), explaining why this result
    /// was judged relevant. Always nil for `search`.
    public let reasoning: String?

    private enum CodingKeys: String, CodingKey {
        case id, content, importance, tags
        case memoryType = "memory_type"
        case createdAt = "created_at"
        case sourceSession = "source_session"
        case similarity
        case decayScore = "decay_score"
        case reasoning
    }
}

/// A single subject–predicate–object fact in the knowledge graph.
public struct KnowledgeGraphTriple: Codable, Sendable, Hashable {
    public let subject: String
    public let predicate: String
    public let object: String
}

/// A contradiction detected during consolidation between a newly observed
/// fact and an existing memory.
public struct Contradiction: Codable, Sendable {
    public let existingMemoryId: UUID
    public let newContent: String
    public let existingContent: String
    public let explanation: String

    private enum CodingKeys: String, CodingKey {
        case existingMemoryId = "existing_memory_id"
        case newContent = "new_content"
        case existingContent = "existing_content"
        case explanation
    }
}

/// The result of running `Memory.consolidate` over a session.
public struct ConsolidationReport: Codable, Sendable {
    public let sessionId: String
    public let newFacts: Int
    public let updatedFacts: Int
    public let contradictions: [Contradiction]
    public let knowledgeGraphUpdates: [KnowledgeGraphTriple]

    private enum CodingKeys: String, CodingKey {
        case sessionId = "session_id"
        case newFacts = "new_facts"
        case updatedFacts = "updated_facts"
        case contradictions
        case knowledgeGraphUpdates = "knowledge_graph_updates"
    }
}

/// A tracked session, as returned by `Memory.startSession`,
/// `Memory.getSession`, and `Memory.listSessions`.
///
/// Note: `startedAt`/`endedAt` are decoded from the engine's RFC 3339
/// string representation, not native epoch timestamps.
public struct Session: Codable, Sendable, Identifiable {
    public let id: String
    public let project: String
    public let startedAt: Date
    public let endedAt: Date?
    public let consolidated: Bool
    public let memoryCount: Int

    private enum CodingKeys: String, CodingKey {
        case id, project
        case startedAt = "started_at"
        case endedAt = "ended_at"
        case consolidated
        case memoryCount = "memory_count"
    }

    /// True if the session has not yet been ended.
    public var isActive: Bool { endedAt == nil }
}

/// Shared JSON decoding/encoding configured for the date formats the
/// rememhq-core engine actually produces (RFC 3339 / ISO 8601 with
/// fractional seconds, as emitted by `chrono`'s `to_rfc3339()` and serde's
/// default `DateTime<Utc>` serialization).
enum RememCoding {
    static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .custom { decoder in
            let container = try decoder.singleValueContainer()
            let string = try container.decode(String.self)
            if let date = isoFormatterWithFractional.date(from: string) {
                return date
            }
            if let date = isoFormatter.date(from: string) {
                return date
            }
            throw DecodingError.dataCorruptedError(
                in: container,
                debugDescription: "Unrecognized date format: \(string)"
            )
        }
        return decoder
    }()

    private static let isoFormatterWithFractional: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()

    private static let isoFormatter: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        return formatter
    }()
}
