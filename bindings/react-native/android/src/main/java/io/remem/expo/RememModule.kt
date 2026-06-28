package io.remem.expo

import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import org.json.JSONArray
import org.json.JSONObject

/**
 * Android side of the React Native binding over rememhq-core, via the
 * Expo Modules API. Mirrors `RememModule.swift`'s `AsyncFunction` surface
 * exactly — see that file's doc comment for the full design rationale
 * (the engineId-keying scheme, the -1 importance sentinel, etc.), which
 * applies here unchanged.
 *
 * Unlike iOS, where `EngineHandle`'s C interop required manually
 * decoding raw JSON `Data` via `JSONSerialization`, Kotlin's
 * `org.json.JSONObject`/`JSONArray` are natively supported return types
 * for Expo's Kotlin bridge (confirmed in Expo's own native-module
 * tutorial), so [NativeBridge]'s raw JSON `String` results are simply
 * wrapped in a `JSONObject`/`JSONArray` constructor call here, with no
 * separate decode step needed.
 *
 * Error surfacing: every `NativeBridge` function throws a plain
 * `RuntimeException` (from the JNI layer, via `env.throw_new`) on
 * failure. Per Expo's documented behavior, an `AsyncFunction` body that
 * throws rejects the JS promise automatically — same mechanism, same
 * caveat about `Exception`/`.code` not being used here, as
 * `RememModule.swift`'s own doc comment explains for iOS.
 */
class RememModule : Module() {
  override fun definition() = ModuleDefinition {
    Name("Remem")

    // -----------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------

    AsyncFunction("openEngine") { project: String, dataDir: String? ->
      NativeBridge.openEngine(project, dataDir)
    }

    AsyncFunction("closeEngine") { engineId: Long ->
      NativeBridge.closeEngine(engineId)
    }

    // -----------------------------------------------------------------
    // Memory operations
    // -----------------------------------------------------------------

    AsyncFunction("store") { engineId: Long, content: String, tagsJson: String?, importance: Double ->
      JSONObject(NativeBridge.store(engineId, content, tagsJson, importance.toFloat()))
    }

    AsyncFunction("recall") { engineId: Long, query: String, limit: Int, filterTagsJson: String? ->
      JSONArray(NativeBridge.recall(engineId, query, limit, filterTagsJson))
    }

    AsyncFunction("search") { engineId: Long, query: String, limit: Int, filterTagsJson: String? ->
      JSONArray(NativeBridge.search(engineId, query, limit, filterTagsJson))
    }

    AsyncFunction("update") {
        engineId: Long,
        id: String,
        content: String?,
        importance: Double,
        tagsJson: String?,
      ->
      JSONObject(NativeBridge.update(engineId, id, content, importance.toFloat(), tagsJson))
    }

    AsyncFunction("forget") { engineId: Long, id: String, mode: String ->
      NativeBridge.forget(engineId, id, mode)
    }

    AsyncFunction("decay") { engineId: Long, factor: Double ->
      NativeBridge.decay(engineId, factor.toFloat())
    }

    // -----------------------------------------------------------------
    // Knowledge graph
    // -----------------------------------------------------------------

    AsyncFunction("queryKnowledge") {
        engineId: Long,
        subject: String?,
        predicate: String?,
        objectValue: String?,
      ->
      JSONArray(NativeBridge.queryKnowledge(engineId, subject, predicate, objectValue))
    }

    AsyncFunction("entityContext") { engineId: Long, entity: String ->
      JSONArray(NativeBridge.entityContext(engineId, entity))
    }

    // -----------------------------------------------------------------
    // Sessions
    // -----------------------------------------------------------------

    AsyncFunction("startSession") { engineId: Long, id: String ->
      NativeBridge.startSession(engineId, id)
    }

    AsyncFunction("endSession") { engineId: Long, id: String ->
      NativeBridge.endSession(engineId, id)
    }

    AsyncFunction("getSession") { engineId: Long, id: String ->
      NativeBridge.getSession(engineId, id)?.let { JSONObject(it) }
    }

    AsyncFunction("listSessions") { engineId: Long, limit: Int ->
      JSONArray(NativeBridge.listSessions(engineId, limit))
    }

    AsyncFunction("consolidate") { engineId: Long, sessionId: String, model: String? ->
      JSONObject(NativeBridge.consolidate(engineId, sessionId, model))
    }
  }
}
