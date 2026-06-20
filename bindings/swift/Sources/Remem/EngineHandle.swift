import CRemem
import Foundation

/// Wraps a `remem_engine_t*` pointer and contains every direct call into
/// the C ABI. Nothing outside this file should touch `CRemem` symbols
/// directly — `Memory` (the public API) only sees safe Swift types.
///
/// Not `Sendable` by inheritance (it's a raw pointer), but synchronization
/// is handled by `Memory` being an `actor`: only one task can call into
/// this handle's methods at a time, which is sufficient since the engine
/// pointer itself is freely shared by remem's own internal Tokio runtime
/// across threads (see rememhq.h's thread-safety note).
final class EngineHandle {
    private let pointer: OpaquePointer

    init(project: String, dataDir: String?) throws {
        var errorPtr: UnsafeMutablePointer<CChar>?

        let createdPointer: OpaquePointer? = project.withCString { projectCString in
            if let dataDir {
                return dataDir.withCString { dataDirCString in
                    remem_engine_new(projectCString, dataDirCString, &errorPtr)
                }
            } else {
                return remem_engine_new(projectCString, nil, &errorPtr)
            }
        }

        if let createdPointer {
            self.pointer = createdPointer
        } else {
            throw RememError.engineInitFailed(Self.takeErrorString(errorPtr) ?? "unknown error")
        }
    }

    deinit {
        remem_engine_free(pointer)
    }

    // MARK: - Memory operations

    func store(content: String, tagsJSON: String?, importance: Float) throws -> Data {
        try call { errorPtr in
            content.withCString { contentCString in
                withOptionalCString(tagsJSON) { tagsCString in
                    remem_store(pointer, contentCString, tagsCString, importance, &errorPtr)
                }
            }
        }
    }

    func recall(query: String, limit: Int, filterTagsJSON: String?) throws -> Data {
        try call { errorPtr in
            query.withCString { queryCString in
                withOptionalCString(filterTagsJSON) { tagsCString in
                    remem_recall(pointer, queryCString, limit, tagsCString, &errorPtr)
                }
            }
        }
    }

    func search(query: String, limit: Int, filterTagsJSON: String?) throws -> Data {
        try call { errorPtr in
            query.withCString { queryCString in
                withOptionalCString(filterTagsJSON) { tagsCString in
                    remem_search(pointer, queryCString, limit, tagsCString, &errorPtr)
                }
            }
        }
    }

    func update(
        id: String, content: String?, importance: Float?, tagsJSON: String?
    ) throws -> Data {
        try call { errorPtr in
            id.withCString { idCString in
                withOptionalCString(content) { contentCString in
                    withOptionalCString(tagsJSON) { tagsCString in
                        remem_update(
                            pointer, idCString, contentCString,
                            importance ?? -1, tagsCString, &errorPtr
                        )
                    }
                }
            }
        }
    }

    func forget(id: String, mode: ForgetMode) throws -> Bool {
        var errorPtr: UnsafeMutablePointer<CChar>?
        let found = id.withCString { idCString in
            mode.rawValue.withCString { modeCString in
                remem_forget(pointer, idCString, modeCString, &errorPtr)
            }
        }
        if let message = Self.takeErrorString(errorPtr) {
            throw RememError.engine(message)
        }
        return found
    }

    func decay(factor: Float) throws -> Int {
        var errorPtr: UnsafeMutablePointer<CChar>?
        let count = remem_decay(pointer, factor, &errorPtr)
        if let message = Self.takeErrorString(errorPtr) {
            throw RememError.engine(message)
        }
        return Int(count)
    }

    // MARK: - Knowledge graph

    func queryKnowledge(subject: String?, predicate: String?, object: String?) throws -> Data {
        try call { errorPtr in
            withOptionalCString(subject) { subjectCString in
                withOptionalCString(predicate) { predicateCString in
                    withOptionalCString(object) { objectCString in
                        remem_query_knowledge(
                            pointer, subjectCString, predicateCString, objectCString, &errorPtr
                        )
                    }
                }
            }
        }
    }

    func entityContext(entity: String) throws -> Data {
        try call { errorPtr in
            entity.withCString { entityCString in
                remem_get_entity_context(pointer, entityCString, &errorPtr)
            }
        }
    }

    // MARK: - Sessions

    func createSession(id: String) throws {
        var errorPtr: UnsafeMutablePointer<CChar>?
        let ok = id.withCString { idCString in
            remem_session_create(pointer, idCString, &errorPtr)
        }
        if let message = Self.takeErrorString(errorPtr) {
            throw RememError.engine(message)
        }
        if !ok {
            throw RememError.engine("Failed to create session '\(id)'")
        }
    }

    func endSession(id: String) throws -> Bool {
        var errorPtr: UnsafeMutablePointer<CChar>?
        let found = id.withCString { idCString in
            remem_session_end(pointer, idCString, &errorPtr)
        }
        if let message = Self.takeErrorString(errorPtr) {
            throw RememError.engine(message)
        }
        return found
    }

    /// Returns `nil` if no session with this ID exists.
    func getSession(id: String) throws -> Data? {
        var errorPtr: UnsafeMutablePointer<CChar>?
        let resultPtr = id.withCString { idCString in
            remem_session_get(pointer, idCString, &errorPtr)
        }
        if let message = Self.takeErrorString(errorPtr) {
            throw RememError.engine(message)
        }
        guard let resultPtr else { return nil }
        defer { remem_free_string(resultPtr) }
        return Data(String(cString: resultPtr).utf8)
    }

    func listSessions(limit: Int) throws -> Data {
        try call { errorPtr in
            remem_session_list(pointer, limit, &errorPtr)
        }
    }

    func consolidate(sessionId: String, model: String?) throws -> Data {
        try call { errorPtr in
            sessionId.withCString { sessionIdCString in
                withOptionalCString(model) { modelCString in
                    remem_consolidate(pointer, sessionIdCString, modelCString, &errorPtr)
                }
            }
        }
    }

    // MARK: - Shared call plumbing

    /// Runs `body`, which must invoke exactly one `remem_*` function that
    /// returns an owned `char*` and writes through the `errorPtr` it's
    /// given, then converts the result to `Data` and frees the native
    /// string. Throws `RememError.engine` if the call reported an error,
    /// or if it returned null without an error (treated as "engine
    /// returned no data" — this should not normally happen for the
    /// functions that route through this helper, since none of them have
    /// a legitimate "not found" null case the way `getSession` does).
    private func call(
        _ body: (inout UnsafeMutablePointer<CChar>?) -> UnsafeMutablePointer<CChar>?
    ) throws -> Data {
        var errorPtr: UnsafeMutablePointer<CChar>?
        let resultPtr = body(&errorPtr)

        if let message = Self.takeErrorString(errorPtr) {
            throw RememError.engine(message)
        }

        guard let resultPtr else {
            throw RememError.engine("Engine returned no data and no error")
        }

        defer { remem_free_string(resultPtr) }
        return Data(String(cString: resultPtr).utf8)
    }

    /// Consumes and frees an error pointer set by a `remem_*` call,
    /// returning its string contents if it was non-null.
    private static func takeErrorString(_ ptr: UnsafeMutablePointer<CChar>?) -> String? {
        guard let ptr else { return nil }
        defer { remem_free_string(ptr) }
        return String(cString: ptr)
    }
}

/// Calls `body` with `string` converted to a C string, or with `nil` if
/// `string` is `nil`. Mirrors `String.withCString` but threads the
/// optionality through, since the FFI layer treats null pointers as a
/// meaningful "absent" value (e.g. "no tag filter", "use the default
/// model") rather than an error.
private func withOptionalCString<R>(
    _ string: String?, _ body: (UnsafePointer<CChar>?) -> R
) -> R {
    guard let string else { return body(nil) }
    return string.withCString(body)
}
