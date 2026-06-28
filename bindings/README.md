# remem Bindings

This directory contains the language-specific bindings that expose the `rememhq-core` native engine (written in Rust/C++) to higher-level languages and frameworks.

## Current Bindings

### React Native / Expo (`react-native/`)
- **Status:** 🚧 In-progress
- **Description:** On-device React Native binding using the Expo Modules API. Allows running the reasoning engine directly inside an iOS or Android app without needing a background service.
- **Progress:** TypeScript API, Swift (iOS), and JNI+Rust (Android) layers are implemented and compile successfully. They are ready for runtime testing and verification.

### Swift (`swift/`)
- **Status:** 🚧 In-progress
- **Description:** A Swift Package bridging to the native engine, providing a native `Memory` actor for iOS/macOS apps.
- **Progress:** API and bridging header implemented, but not yet verified against a running engine in Xcode.

### Python / TypeScript Server SDKs
*Note: The HTTP-based SDKs for the server API are located in `../sdk/`, not here. This directory is strictly for direct native bindings.*

## Planned Bindings
- **Python (Native):** Direct `pyo3` or `ctypes` binding to run the engine in a Python process without the HTTP overhead. (Not yet started).
- **Node.js (Native):** N-API binding for direct Node.js execution. (Not yet started).
