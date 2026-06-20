import Foundation

/// An error surfaced from the underlying remem engine, either from Rust
/// (anyhow error strings propagated across the FFI boundary) or from this
/// Swift wrapper itself (encoding/decoding failures, misuse).
public enum RememError: Error, CustomStringConvertible, Equatable {
    /// The native engine reported a failure. The associated string is the
    /// human-readable message produced on the Rust side.
    case engine(String)

    /// `Memory.open` was given a project or data directory that could not
    /// be turned into a valid engine.
    case engineInitFailed(String)

    /// A JSON payload returned by the engine could not be decoded into the
    /// expected Swift type. This indicates a bug (a mismatch between this
    /// binding's models and the engine's actual JSON shape) rather than a
    /// normal runtime failure.
    case decodingFailed(String)

    /// A request could not be encoded to send across the FFI boundary
    /// (e.g. tags containing data that isn't valid JSON/UTF-8).
    case encodingFailed(String)

    public var description: String {
        switch self {
        case .engine(let message):
            return message
        case .engineInitFailed(let message):
            return "Failed to initialize remem engine: \(message)"
        case .decodingFailed(let message):
            return "Failed to decode engine response: \(message)"
        case .encodingFailed(let message):
            return "Failed to encode request: \(message)"
        }
    }
}
