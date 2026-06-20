# Swift Binding

On-device Swift Package wrapping `rememhq-core`'s native engine via its C
ABI — no `remem serve` instance required. Built for iOS and macOS.

## Status

🚧 **Early / in-progress.** The core `Memory` actor API works end-to-end
against a real local engine (SQLite storage + HNSW vector index), with CI
verifying it on every change. It hasn't shipped as a versioned release,
and the XCFramework distribution path exists as a script but hasn't been
run-tested on real Apple hardware. Treat this as a working development
snapshot, not a stable public API.

**Implemented:**
- `store`, `recall`, `search`, `update`, `forget`, `decay`
- `queryKnowledge`, `entityContext`
- `startSession`, `endSession`, `getSession`, `listSessions`, `consolidate`
- Full `Codable` models mirroring the engine's JSON shapes
  (`MemoryRecord`, `MemoryResult`, `Session`, `ConsolidationReport`, etc.)
- CI on `macos-latest` builds `rememhq-core`, builds the Swift package,
  and runs the full test suite against a real (mock-provider) engine —
  see `.github/workflows/bindings-swift.yml`
- A script to assemble a real XCFramework (`scripts/build-xcframework.sh`)
  — see "Distributing as an XCFramework" below. **Caveat:** this script
  has been reviewed carefully and is believed correct, but it hasn't
  been run end-to-end (no macOS/Xcode environment was available while
  writing it). Treat it as a strong starting point, not a guarantee.

**Not yet implemented:**
- `Package.swift` still links against a local `cargo build` output
  directory by default rather than the XCFramework above — switching
  the default over needs the script run-tested on real hardware first
  (see the script's own closing instructions for the manual swap)
- CoreML execution provider for on-device embeddings (the engine still
  needs a configured reasoning/embedding provider — Anthropic, OpenAI,
  Google, or `local` pointed at an Ollama/llama.cpp endpoint; there's no
  Apple Silicon-native embedding path yet)
- AppKit menu bar integration for macOS
- Whether `tokio`'s full feature set behaves correctly under iOS's
  sandboxed threading model hasn't been verified on a real device or
  simulator — CI only exercises the macOS host target

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
├── Tests/RememTests/
└── scripts/
    ├── sync-header.sh          — keep the vendored C header in sync
    └── build-xcframework.sh    — assemble a real XCFramework (macOS-only)
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

## Distributing as an XCFramework

The local-cargo-build linking approach above is great for iterating on
this repo, but it's not how a real app should depend on this package —
that requires checking out the whole monorepo and running `cargo build`
before every `swift build`. `scripts/build-xcframework.sh` assembles a
proper `RememHQCore.xcframework` instead: it cross-compiles
`rememhq-core` for macOS (arm64 + x86_64), iOS device (arm64), and iOS
simulator (arm64 + x86_64), `lipo`s each platform's architectures into a
single fat static library, bakes in a `module.modulemap` so the result
is still importable as `CRemem`, and bundles it all with `xcodebuild
-create-xcframework`.

```sh
# Run on macOS with Xcode (not just the Command Line Tools) installed
rustup target add aarch64-apple-darwin x86_64-apple-darwin \
                   aarch64-apple-ios aarch64-apple-ios-sim \
                   x86_64-apple-ios-sim
bindings/swift/scripts/build-xcframework.sh
```

This produces `bindings/swift/build/RememHQCore.xcframework`. It is **not
yet wired into `Package.swift`** — switching the default linking
strategy over to it needs to be run-tested on real macOS/iOS hardware
first (this script was written and carefully reviewed, but couldn't be
executed in the environment it was authored in). The script's own
closing output explains the exact `Package.swift` changes needed once
you've verified it builds correctly for you: swapping the `CRemem`
source target for a `.binaryTarget` pointing at the xcframework, and
adding an explicit `-lc++` link flag to the `Remem` target (static
linking needs this explicitly; the current dynamic-library setup
resolves it automatically at load time).

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
