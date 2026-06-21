// swift-tools-version:5.9
import PackageDescription

// MARK: - Linking against the Rust-built rememhq-core library
//
// rememhq-core compiles to `librememhq_core.{dylib,a}` via a standard cargo
// build (crate-type = ["cdylib", "staticlib", "rlib"] in
// rememhq-core/Cargo.toml). There is no published XCFramework yet (see
// bindings/swift/README.md for the roadmap), so for now this package links
// against a local cargo build output directory.
//
// Before building this package, run from the repo root:
//   cargo build --release -p rememhq-core
//
// which produces target/release/librememhq_core.dylib (macOS) or
// target/release/librememhq_core.so (Linux, for local iteration only —
// this package targets iOS/macOS for actual distribution).
//
// REMEM_LIB_DIR can override the search path, e.g. when this package is
// checked out standalone and the dylib has been copied alongside it:
//   REMEM_LIB_DIR=/path/to/libs swift build
import Foundation

let libDir = ProcessInfo.processInfo.environment["REMEM_LIB_DIR"]
    ?? "../../target/release"

let package = Package(
    name: "Remem",
    platforms: [
        .iOS(.v15),
        .macOS(.v12),
    ],
    products: [
        .library(name: "Remem", targets: ["Remem"]),
    ],
    targets: [
        // Low-level C shim exposing rememhq-core's C ABI (rememhq.h) as an
        // importable Clang module. No C sources of its own — the actual
        // implementation lives in the Rust-built native library linked in
        // below.
        .target(
            name: "CRemem",
            path: "Sources/CRemem",
            sources: [],
            publicHeadersPath: "include"
        ),

        // High-level, ergonomic Swift API. This is what consumers import.
        .target(
            name: "Remem",
            dependencies: ["CRemem"],
            path: "Sources/Remem",
            linkerSettings: [
                .unsafeFlags([
                    "-L\(libDir)",
                    "-lrememhq_core",
                    // libremem's C++ sources are compiled into
                    // rememhq-core. The cdylib path (the default, used on
                    // macOS) resolves this automatically at load time, but
                    // any static-linking path (e.g. the iOS Simulator CI
                    // job, which isolates librememhq_core.a to dodge
                    // Apple's dylib-over-.a linker preference) needs it
                    // explicit. Harmless no-op for the dynamic path.
                    "-lc++",
                ])
            ]
        ),

        .testTarget(
            name: "RememTests",
            dependencies: ["Remem"],
            path: "Tests/RememTests",
            linkerSettings: [
                .unsafeFlags([
                    "-L\(libDir)",
                    "-lrememhq_core",
                    "-lc++",
                ])
            ]
        ),
    ]
)
