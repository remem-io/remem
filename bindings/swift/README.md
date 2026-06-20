# Swift Binding

On-device Swift Package wrapping `rememhq-core`'s native engine via its C
ABI — no `remem serve` instance required. Built for iOS and macOS.

## Status

🚧 **Early / in-progress.** The core `Memory` actor API works end-to-end
against a real local engine (SQLite storage + HNSW vector index), but this
hasn't shipped as a versioned release or compiled XCFramework yet. Treat
this as a working development snapshot, not a stable public API.

**Implemented:**
- `store`, `recall`, `search`, `update`, `forget`, `decay`
- `queryKnowledge`, `entityContext`
- `startSession`, `endSession`, `getSession`, `listSessions`, `consolidate`
- Full `Codable` models mirroring the engine's JSON shapes
  (`MemoryRecord`, `MemoryResult`, `Session`, `ConsolidationReport`, etc.)

**Not yet implemented:**
- Compiled XCFramework / binary distribution (see "Linking" below — for
  now this links against a local `cargo build` output directory)
- CoreML execution provider for on-device embeddings (the engine still
  needs a configured reasoning/embedding provider — Anthropic, OpenAI,
  Google, or `local` pointed at an Ollama/llama.cpp endpoint; there's no
  Apple Silicon-native embedding path yet)
- AppKit menu bar integration for macOS
- CI coverage (no GitHub Actions workflow builds/tests this package yet)

## Architecture

```
bindings/swift/
├── Package.swift
├── Sources/
│   ├── CRemem/             # C target: just a module map over rememhq.h
│   └── Remem/              # Swift target: the public API
│       ├── Memory.swift           — public actor, the main entry point
│       ├── MemoryModels.swift     — Codable structs (MemoryRecord, Session, ...)
│       ├── EngineHandle.swift     — all unsafe FFI calls live here
│       └── RememError.swift
└── Tests/RememTests/
```

`Memory` is a Swift `actor`, so calls are serialized per-instance and it's
safe to share one across concurrent tasks. All `unsafe` pointer handling
is contained in `EngineHandle` — nothing else in the package touches
`CRemem` symbols directly.

## Linking

There's no published XCFramework yet, so this package currently links
against a local `cargo build` output directory rather than a vendored
binary. Build the native library first:

```sh
# from the repo root
cargo build --release -p rememhq-core
```

This produces `target/release/librememhq_core.dylib`. `Package.swift`
defaults to looking for it at `../../target/release` (relative to
`bindings/swift/`), which matches the standard cargo workspace layout. If
you've built elsewhere, or are working with this package checked out
standalone, override the search path:

```sh
REMEM_LIB_DIR=/path/to/libs swift build
```

**Known limitation:** the `Remem` target's linker settings use
`.unsafeFlags`, which means this package currently can't be added as a
dependency *of another Swift package* (SwiftPM disallows that for
packages using unsafe flags, to keep builds reproducible). It works fine
as a direct dependency of an app target (Xcode project or app-level
`Package.swift`). This goes away once an XCFramework binary target
replaces the local-build linking approach.

## Building and testing

```sh
cargo build --release -p rememhq-core   # from repo root
cd bindings/swift
swift build
swift test
```

Tests run against the real engine with `REMEM_PROVIDER=mock` and an
isolated temp `REMEM_DATA_DIR` per test — no API keys or network access
required, and no shared state between test runs.

## Keeping the vendored header in sync

The C header (`Sources/CRemem/include/rememhq.h`) is a vendored copy of
the canonical `rememhq-core/include/rememhq.h`, so this package can build
standalone without checking out the whole monorepo. It's hand-maintained
(not `cbindgen`-generated — see the header's own doc comment for why). If
you change the FFI surface in `rememhq-core/src/ffi/mod.rs`, update
`rememhq-core/include/rememhq.h` first, then run:

```sh
bindings/swift/scripts/sync-header.sh
```

## Usage

```swift
import Remem

let memory = try Memory.open(project: "my-agent")

let record = try await memory.store(
    "User prefers dark mode",
    tags: ["preferences"]
)

let results = try await memory.recall("what are the user's preferences?")
for result in results {
    print(result.content, result.similarity)
}

try await memory.startSession(id: "conversation-42")
// ... store more memories with this session active ...
try await memory.endSession(id: "conversation-42")
let report = try await memory.consolidate(sessionId: "conversation-42")
print("Extracted \(report.newFacts) new facts")
```

By default, `Memory.open` resolves configuration the same way the CLI
does (`.remem/config.toml`, falling back to environment variables like
`ANTHROPIC_API_KEY` / `REMEM_PROVIDER`). Pass `dataDir:` to point at a
specific config/storage directory — useful for app sandboxing on iOS,
where you'll typically want something under
`FileManager.default.urls(for: .applicationSupportDirectory, ...)`.
