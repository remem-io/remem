import XCTest

@testable import Remem

/// These tests link against the real `rememhq-core` native library and
/// exercise the full FFI round-trip — they are integration tests, not
/// pure-Swift unit tests. Each test gets an isolated `REMEM_DATA_DIR`
/// (a fresh temp directory) and forces `REMEM_PROVIDER=mock` so no
/// network calls or API keys are required.
///
/// Build the native library before running these tests:
///   cargo build --release -p rememhq-core
///   cd bindings/swift && swift test
final class MemoryTests: XCTestCase {
    private var tempDir: URL!

    override func setUpWithError() throws {
        tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("remem-swift-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)

        setenv("REMEM_DATA_DIR", tempDir.path, 1)
        setenv("REMEM_PROVIDER", "mock", 1)
        setenv("REMEM_REASONING_PROVIDER", "mock", 1)
        setenv("REMEM_EMBEDDING_PROVIDER", "mock", 1)
    }

    override func tearDownWithError() throws {
        if let tempDir {
            try? FileManager.default.removeItem(at: tempDir)
        }
        unsetenv("REMEM_DATA_DIR")
        unsetenv("REMEM_PROVIDER")
        unsetenv("REMEM_REASONING_PROVIDER")
        unsetenv("REMEM_EMBEDDING_PROVIDER")
    }

    /// Each test uses its own project name (in addition to its own temp
    /// `REMEM_DATA_DIR`) so parallel test runs never share a SQLite file.
    private func openMemory(project: String = #function) throws -> Memory {
        try Memory.open(project: project)
    }

    func testStoreReturnsRecordWithMatchingContent() async throws {
        let memory = try openMemory()
        let record = try await memory.store("The user's favorite color is teal", tags: ["preferences"])

        XCTAssertEqual(record.content, "The user's favorite color is teal")
        XCTAssertEqual(record.tags, ["preferences"])
        XCTAssertEqual(record.memoryType, .fact)
        XCTAssertGreaterThanOrEqual(record.importance, 1)
        XCTAssertLessThanOrEqual(record.importance, 10)
    }

    func testStoreWithExplicitImportance() async throws {
        let memory = try openMemory()
        let record = try await memory.store("Critical deploy credential rotated", importance: 9)

        XCTAssertEqual(record.importance, 9)
    }

    func testSearchFindsStoredMemory() async throws {
        let memory = try openMemory()
        _ = try await memory.store("Remem supports Swift bindings", tags: ["project"])

        let results = try await memory.search("Swift bindings")

        XCTAssertFalse(results.isEmpty)
        XCTAssertTrue(results.contains { $0.content.contains("Swift bindings") })
    }

    func testUpdateChangesOnlySpecifiedFields() async throws {
        let memory = try openMemory()
        let original = try await memory.store("Draft note", tags: ["draft"], importance: 3)

        let updated = try await memory.update(id: original.id, content: "Final note")

        XCTAssertEqual(updated.id, original.id)
        XCTAssertEqual(updated.content, "Final note")
        // Tags and importance should be unchanged since we didn't pass them.
        XCTAssertEqual(updated.tags, ["draft"])
        XCTAssertEqual(updated.importance, 3)
    }

    func testUpdateWithEmptyTagsArrayClearsTags() async throws {
        let memory = try openMemory()
        let original = try await memory.store("Tagged note", tags: ["a", "b"])

        // Passing an empty array is different from passing nil: it clears
        // tags rather than leaving them unchanged.
        let updated = try await memory.update(id: original.id, tags: [])

        XCTAssertEqual(updated.tags, [])
    }

    func testForgetRemovesMemory() async throws {
        let memory = try openMemory()
        let record = try await memory.store("Temporary scratch note")

        let found = try await memory.forget(id: record.id)
        XCTAssertTrue(found)

        // Forgetting an already-removed (or never-existing) ID returns false.
        let foundAgain = try await memory.forget(id: record.id)
        XCTAssertFalse(foundAgain)
    }

    func testForgetUnknownIdReturnsFalse() async throws {
        let memory = try openMemory()
        let found = try await memory.forget(id: UUID())
        XCTAssertFalse(found)
    }

    func testSessionLifecycle() async throws {
        let memory = try openMemory()
        let sessionId = "session-\(UUID().uuidString)"

        try await memory.startSession(id: sessionId)

        let fetched = try await memory.getSession(id: sessionId)
        XCTAssertNotNil(fetched)
        XCTAssertEqual(fetched?.id, sessionId)
        XCTAssertTrue(fetched?.isActive ?? false)

        let ended = try await memory.endSession(id: sessionId)
        XCTAssertTrue(ended)

        let afterEnd = try await memory.getSession(id: sessionId)
        XCTAssertFalse(afterEnd?.isActive ?? true)
    }

    func testGetSessionReturnsNilForUnknownId() async throws {
        let memory = try openMemory()
        let session = try await memory.getSession(id: "does-not-exist-\(UUID().uuidString)")
        XCTAssertNil(session)
    }

    func testEndSessionReturnsFalseForUnknownId() async throws {
        let memory = try openMemory()
        let ended = try await memory.endSession(id: "does-not-exist-\(UUID().uuidString)")
        XCTAssertFalse(ended)
    }

    func testListSessionsIncludesStartedSession() async throws {
        let memory = try openMemory()
        let sessionId = "session-\(UUID().uuidString)"
        try await memory.startSession(id: sessionId)

        let sessions = try await memory.listSessions()

        XCTAssertTrue(sessions.contains { $0.id == sessionId })
    }

    func testDecayReturnsNonNegativeCount() async throws {
        let memory = try openMemory()
        _ = try await memory.store("A memory that might decay")

        let archivedCount = try await memory.decay()

        XCTAssertGreaterThanOrEqual(archivedCount, 0)
    }

    func testQueryKnowledgeWithNoMatchesReturnsEmpty() async throws {
        let memory = try openMemory()
        let triples = try await memory.queryKnowledge(subject: "nonexistent-entity-xyz")
        XCTAssertTrue(triples.isEmpty)
    }
}
