# libremem-sys

[![Crates.io](https://img.shields.io/crates/v/libremem-sys.svg)](https://crates.io/crates/libremem-sys)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

Low-level C++ FFI bindings for **remem**, the reasoning memory layer for AI agents.

This internal crate builds the C++ backend for the vector index (using HNSW) and the ONNX embedding engine, exposing them to Rust via a safe C interface. It is consumed by `rememhq-core` and is generally not meant to be used directly by external applications.

## Dependencies

- C++17 compatible compiler
- `onnxruntime` (automatically downloaded/linked depending on features)

## Building

This crate uses `cc` via a `build.rs` script to compile the underlying C++ source files found in `src/`. No standalone CMake is required.

## License

Apache License 2.0. See the [LICENSE](../LICENSE) file.
