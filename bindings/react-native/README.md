# React Native Binding

On-device React Native binding over `rememhq-core`'s native engine, via
the [Expo Modules API](https://docs.expo.dev/modules/overview/) — no
`remem serve` instance required. Mirrors the architecture and API shape
of the [Swift Package binding](../swift), which this binding's iOS side
borrows heavily from.

## Status

🚧 **In-progress.** The TypeScript API, native module wiring, and Swift/JNI implementations are written and compile cleanly. The **Android** implementation uses JNI + Rust via Corrosion and is fully wired up to Expo. However, both platforms still need runtime testing against a real engine (see "What's verified" vs "What's not verified" below).

### Why Expo Modules API, not Nitro Modules

An earlier version of this binding's plan called for Nitro Modules +
JSI. That's been reconsidered: **Expo Go cannot load any custom native
code at all**, regardless of which module technology is used — Nitro,
TurboModules, and the Expo Modules API are all equally unusable from the
published Expo Go app. All three require a [development
build](https://docs.expo.dev/develop/development-builds/introduction/)
(`npx expo run:ios` / `run:android`, or an EAS dev build) instead. Given
that constraint is unavoidable either way, the Expo Modules API was
chosen for being Expo's own first-party path — smoothest fit with
`expo-module.config.json` autolinking, EAS Build, and `create-expo-module`
tooling, at the cost of being somewhat more boilerplate-heavy than Nitro.

### What's verified

- `npm run build` (TypeScript compilation via `tsc`) passes cleanly
- `npm run lint` (ESLint) passes cleanly on `src/` and the example app
- The example app's own `tsc --noEmit` passes cleanly, confirming the
  full `Memory` API is correctly typed and resolves through to the
  native module declarations
- `src/__tests__/Memory.test.ts` (a mocked-native-module unit test suite
  covering every `Memory` method's argument encoding — tag JSON
  encoding, the `-1` importance sentinel, empty-array-clears-vs-
  omitted-leaves-unchanged for `update`'s tags, and every other
  method's default values) **type-checks cleanly** but **could not
  actually be run** in the environment it was written in: `npm run test`
  (via `jest-expo`) crashes with `Super expression must either be null
  or a function` inside `jest-expo`'s unconditional
  `require('expo/src/winter')` fetch-polyfill installation, unrelated to
  anything in this binding's own code. Swapping to the plain
  `react-native` jest preset as a workaround hit a *different* crash (a
  babel/parser syntax error inside `react-native@0.82.1`'s own internals
  under Node 22). Both look like environment/dependency-version
  incompatibilities specific to the sandbox this was developed in, not
  problems with the test code itself — but this needs to actually be run
  successfully somewhere before trusting it.

### What's not verified

- The Swift implementation (`ios/RememModule.swift`,
  `ios/RememCore/EngineHandle.swift`) has not been compiled — no
  macOS/Xcode environment was available while writing it. Treat it with
  the same caution as `bindings/swift`'s own unverified pieces.
- Nothing has been run on a real iOS Simulator, Android emulator, physical device, or in an actual Expo development build.
- Whether `Exception` (Expo's typed-error mechanism, which can attach a
  JS-visible `.code`) is the right replacement for the current "throw a
  plain `RememError`" approach — see the note at the top of
  `RememModule.swift`.

## Architecture

```
bindings/react-native/
├── package.json              — published as @remem-io/react-native
├── expo-module.config.json
├── src/
│   ├── index.ts              — public Memory class (mirrors bindings/swift's Memory actor)
│   ├── Remem.types.ts        — TS types mirroring the engine's JSON shapes
│   ├── RememModule.ts        — raw native module declaration (iOS + Android)
│   └── RememModule.web.ts    — stub; remem has no web/WASM target
├── ios/
│   ├── Remem.podspec
│   ├── RememModule.swift     — Expo module: AsyncFunctions -> EngineHandle
│   └── RememCore/
│       ├── rememhq.h                       — vendored copy of the canonical C header
│       ├── RememCore-Bridging-Header.h      — exposes rememhq.h to Swift in this pod target
│       ├── EngineHandle.swift               — ported from bindings/swift, unsafe FFI calls
│       ├── MemoryModels.swift               — just ForgetMode (see file for why)
│       └── RememError.swift
├── android/
│   ├── build.gradle              — includes Corrosion CMake configuration
│   ├── CMakeLists.txt            — Corrosion bridge to build Rust
│   ├── rust/                     — Rust JNI crate (remem_android_jni) bridging Core and Expo
│   └── src/main/java/...         — Expo Kotlin module (RememModule.kt)
└── example/                      — a real (if minimal) Expo app exercising store/search
```

Unlike `bindings/swift` (a Swift Package, using a `CRemem` module target
+ Clang module map), this binding is built with CocoaPods, so it uses
the conventional **bridging header** approach instead
(`SWIFT_OBJC_BRIDGING_HEADER` in `Remem.podspec`) to expose the same C
ABI to Swift within one pod target.

### Why EngineHandle is duplicated instead of shared

`ios/RememCore/EngineHandle.swift` is a near-verbatim copy of
`bindings/swift/Sources/Remem/EngineHandle.swift`, not a shared
dependency. CocoaPods pod targets can't directly depend on a Swift
Package target, so sharing it would need either vendoring `bindings/swift`
as a local pod (real but more setup than this binding currently
warrants) or extracting the FFI-calling logic into a separate small
package both could depend on. Until one of those feels worth doing,
keeping a synced copy was the pragmatic choice — same tradeoff as
`rememhq.h` itself, which is also vendored rather than referenced by
relative path (see `bindings/scripts/sync-headers.sh`).

Unlike `EngineHandle`, this binding's `Memory`-equivalent layer
(`RememModule.swift`) is **not** a copy of `bindings/swift`'s
`Memory.swift` — it talks to `EngineHandle` directly and decodes
responses with `JSONSerialization` rather than typed `Codable` models,
since only primitives/arrays/dictionaries cross the Expo JS bridge
automatically. The typed model layer lives in TypeScript instead
(`src/Remem.types.ts`), decoded for free by the JS engine once the JSON
crosses the bridge as a plain object.

## Linking (iOS)

Same situation as `bindings/swift`: there's no published
XCFramework/binary yet, so `Remem.podspec` links against a local `cargo
build` output directory.

```sh
# from the repo root
cargo build --release -p rememhq-core
```

`Remem.podspec` looks for the resulting library at `../../../target/release`
(relative to `ios/`, matching the standard cargo workspace layout).
Override with `REMEM_LIB_DIR` if you've built elsewhere — this needs to
be set in the environment `pod install` runs in, e.g.:

```sh
REMEM_LIB_DIR=/path/to/libs npx expo run:ios
```

The podspec links `librememhq_core.a` (the static library; rememhq-core's
`crate-type` includes `staticlib` for exactly this) plus `libc++`
explicitly, since libremem's C++ sources are compiled into it and a
static archive — unlike a dylib — doesn't resolve that automatically at
load time.

## Android

The Android implementation builds a custom Rust crate (`remem_android_jni`) through the Android NDK, integrated natively into the Android build pipeline using [Corrosion](https://github.com/corrosion-rs/corrosion) (a CMake-Cargo bridge).

When you run an Android build (e.g. `npx expo run:android`), Gradle invokes CMake, which in turn invokes Cargo to cross-compile the Rust codebase for the target Android architecture (e.g., `aarch64-linux-android`). The resulting `.so` binary is then automatically packaged in the `aar`.

The `RememModule.kt` Kotlin module accesses the Rust code using a JNI interface configured in `bindings/react-native/android/rust/src/lib.rs`. This JNI interface mirrors the C ABI used on iOS to efficiently pass data without excessive serialization.

## Usage

```ts
import { Memory } from '@remem-io/react-native';

const memory = await Memory.open({ project: 'my-agent' });

const record = await memory.store('User prefers dark mode', {
  tags: ['preferences'],
});

const results = await memory.recall("what are the user's preferences?");
for (const result of results) {
  console.log(result.content, result.similarity);
}

await memory.startSession('conversation-42');
// ... store more memories with this session active ...
await memory.endSession('conversation-42');
const report = await memory.consolidate('conversation-42');
console.log(`Extracted ${report.newFacts} new facts`);

await memory.close();
```

Call `memory.close()` when you're done with an instance (e.g. a
`useEffect` cleanup function) — it releases the native engine handle and
its open SQLite connection. An unclosed `Memory` leaks both for the
lifetime of the app process.

By default, `Memory.open` resolves configuration the same way the CLI
does (`.remem/config.toml`, falling back to environment variables like
`ANTHROPIC_API_KEY` / `REMEM_PROVIDER`). Pass `dataDir` to point at a
specific config/storage directory — on iOS, app sandboxing means you'll
typically want this under your app's documents or library directory
(e.g. via `expo-file-system`'s `documentDirectory`) rather than relying
on the engine's default search path.

## Building and testing

```sh
npm run build   # TypeScript compilation
npm run lint     # ESLint
npm run test     # Unit tests — see "What's verified" above for a caveat
```

Running the example app requires a development build (see "Why Expo
Modules API, not Nitro Modules" above — Expo Go won't work):

```sh
cd example
npm install
npx expo run:android
# Or for iOS (Requires macOS / Xcode):
npx expo run:ios
```

> **Note on Windows / iOS:** If you are developing on a Windows machine, you cannot run `npx expo run:ios` locally. You must either test iOS using a Mac, or use [EAS Build](https://docs.expo.dev/build/introduction/) (`eas build -p ios`) to compile a custom Dev Client in the cloud.
