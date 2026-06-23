import ExpoModulesCore
import Foundation

/// React Native binding over rememhq-core, via the Expo Modules API.
///
/// Mirrors the API shape of `Memory` in `bindings/swift` — store, recall,
/// search, update, forget, decay, queryKnowledge, entityContext, and the
/// session lifecycle — but exposed as `AsyncFunction`s returning plain
/// JSON-serializable dictionaries/arrays rather than typed Swift structs,
/// since only primitives, arrays, dictionaries, and `Record` types cross
/// the JS bridge automatically.
///
/// One `EngineHandle` is created per `Remem` instance on the JS side (see
/// `src/index.ts`), keyed by a JS-side numeric handle ID, so a single app
/// can open multiple projects/engines concurrently if it wants to.
///
/// Error surfacing: throwing a `RememError` (a plain `enum: Error`,
/// shared with the SPM Swift binding) from an `AsyncFunction` body
/// rejects the JS promise, per Expo's documented behavior. Expo's own
/// `Exception` type can attach a JS-visible `.code` in addition to a
/// message, but its exact protocol requirements couldn't be verified
/// against source in the environment this was written in — rather than
/// guess at property names and risk a build break, this throws
/// `RememError` directly (which now conforms to `LocalizedError`, so at
/// least `localizedDescription` carries the real message). Revisit with
/// `Exception` once this has been verified against a real Expo Modules
/// Core checkout.
public class RememModule: Module {
    /// Live engine handles, keyed by an opaque ID handed back to JS.
    /// Access is confined to the modules-core dispatch queue (the same
    /// queue every `AsyncFunction` body in this module runs on), so a
    /// plain dictionary is safe without extra locking.
    private var engines: [Int: EngineHandle] = [:]
    private var nextHandleId = 0

    public func definition() -> ModuleDefinition {
        Name("Remem")

        // MARK: - Lifecycle

        AsyncFunction("openEngine") { (project: String, dataDir: String?) -> Int in
            let handle = try EngineHandle(project: project, dataDir: dataDir)
            let id = self.nextHandleId
            self.nextHandleId += 1
            self.engines[id] = handle
            return id
        }

        AsyncFunction("closeEngine") { (engineId: Int) -> Void in
            self.engines.removeValue(forKey: engineId)
        }

        // MARK: - Memory operations

        AsyncFunction("store") {
            (engineId: Int, content: String, tagsJSON: String?, importance: Double) -> [String: Any] in
            let handle = try self.requireEngine(engineId)
            let data = try handle.store(
                content: content, tagsJSON: tagsJSON, importance: Float(importance)
            )
            return try Self.decodeObject(data)
        }

        AsyncFunction("recall") {
            (engineId: Int, query: String, limit: Int, filterTagsJSON: String?) -> [Any] in
            let handle = try self.requireEngine(engineId)
            let data = try handle.recall(query: query, limit: limit, filterTagsJSON: filterTagsJSON)
            return try Self.decodeArray(data)
        }

        AsyncFunction("search") {
            (engineId: Int, query: String, limit: Int, filterTagsJSON: String?) -> [Any] in
            let handle = try self.requireEngine(engineId)
            let data = try handle.search(query: query, limit: limit, filterTagsJSON: filterTagsJSON)
            return try Self.decodeArray(data)
        }

        AsyncFunction("update") {
            (
                engineId: Int, id: String, content: String?, importance: Double,
                tagsJSON: String?
            ) -> [String: Any] in
            let handle = try self.requireEngine(engineId)
            // importance < 0 is the "leave unchanged" sentinel, matching
            // the convention rememhq.h itself uses for remem_update.
            let importanceArg: Float? = importance < 0 ? nil : Float(importance)
            let data = try handle.update(
                id: id, content: content, importance: importanceArg, tagsJSON: tagsJSON
            )
            return try Self.decodeObject(data)
        }

        AsyncFunction("forget") { (engineId: Int, id: String, mode: String) -> Bool in
            let handle = try self.requireEngine(engineId)
            guard let forgetMode = ForgetMode(rawValue: mode) else {
                throw RememError.engine("Unknown forget mode: '\(mode)'")
            }
            return try handle.forget(id: id, mode: forgetMode)
        }

        AsyncFunction("decay") { (engineId: Int, factor: Double) -> Int in
            let handle = try self.requireEngine(engineId)
            return try handle.decay(factor: Float(factor))
        }

        // MARK: - Knowledge graph

        AsyncFunction("queryKnowledge") {
            (engineId: Int, subject: String?, predicate: String?, object: String?) -> [Any] in
            let handle = try self.requireEngine(engineId)
            let data = try handle.queryKnowledge(subject: subject, predicate: predicate, object: object)
            return try Self.decodeArray(data)
        }

        AsyncFunction("entityContext") { (engineId: Int, entity: String) -> [Any] in
            let handle = try self.requireEngine(engineId)
            let data = try handle.entityContext(entity: entity)
            return try Self.decodeArray(data)
        }

        // MARK: - Sessions

        AsyncFunction("startSession") { (engineId: Int, id: String) -> Void in
            let handle = try self.requireEngine(engineId)
            try handle.createSession(id: id)
        }

        AsyncFunction("endSession") { (engineId: Int, id: String) -> Bool in
            let handle = try self.requireEngine(engineId)
            return try handle.endSession(id: id)
        }

        AsyncFunction("getSession") { (engineId: Int, id: String) -> [String: Any]? in
            let handle = try self.requireEngine(engineId)
            guard let data = try handle.getSession(id: id) else { return nil }
            return try Self.decodeObject(data)
        }

        AsyncFunction("listSessions") { (engineId: Int, limit: Int) -> [Any] in
            let handle = try self.requireEngine(engineId)
            let data = try handle.listSessions(limit: limit)
            return try Self.decodeArray(data)
        }

        AsyncFunction("consolidate") {
            (engineId: Int, sessionId: String, model: String?) -> [String: Any] in
            let handle = try self.requireEngine(engineId)
            let data = try handle.consolidate(sessionId: sessionId, model: model)
            return try Self.decodeObject(data)
        }
    }

    // MARK: - Helpers

    private func requireEngine(_ id: Int) throws -> EngineHandle {
        guard let handle = engines[id] else {
            throw RememError.engine(
                "No open engine with handle \(id) (was it already closed?)"
            )
        }
        return handle
    }

    /// Decodes JSON `Data` (always a UTF-8 JSON object or array, per
    /// rememhq.h's documented return conventions) into a JS-bridgeable
    /// dictionary. Deliberately bypasses Memory's typed Codable decoding
    /// — only primitives/arrays/dictionaries cross the Expo bridge
    /// automatically, so going through JSONSerialization instead of
    /// RememCoding.decoder is the right layer here, not a shortcut.
    private static func decodeObject(_ data: Data) throws -> [String: Any] {
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw RememError.decodingFailed(
                "Expected a JSON object, got: \(String(data: data, encoding: .utf8) ?? "<non-utf8>")"
            )
        }
        return object
    }

    private static func decodeArray(_ data: Data) throws -> [Any] {
        guard let array = try JSONSerialization.jsonObject(with: data) as? [Any] else {
            throw RememError.decodingFailed(
                "Expected a JSON array, got: \(String(data: data, encoding: .utf8) ?? "<non-utf8>")"
            )
        }
        return array
    }
}
