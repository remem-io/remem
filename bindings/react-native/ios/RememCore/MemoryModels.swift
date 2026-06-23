import Foundation

/// How a memory should be removed via `EngineHandle.forget`.
///
/// This is the one small piece of `bindings/swift`'s `MemoryModels.swift`
/// reused here. Everything else in that file (the full Codable model set
/// — `MemoryRecord`, `MemoryResult`, `Session`, etc. — and the custom
/// `RememCoding` date strategy) is deliberately NOT duplicated in this
/// binding: `RememModule.swift` decodes engine responses with plain
/// `JSONSerialization` instead of typed Codable models, since only
/// primitives/arrays/dictionaries cross the Expo JS bridge automatically
/// — see `RememModule.swift`'s `decodeObject`/`decodeArray` helpers.
enum ForgetMode: String {
    case delete
    case decay
    case archive
}
