require 'json'

package = JSON.parse(File.read(File.join(__dir__, '..', 'package.json')))

Pod::Spec.new do |s|
  s.name           = 'Remem'
  s.version        = package['version']
  s.summary        = 'On-device reasoning memory layer for AI agents — Expo Modules binding over rememhq-core'
  s.description    = <<-DESC
    React Native binding (via the Expo Modules API) for remem's native
    reasoning engine. Wraps rememhq-core's C ABI directly — no remem
    serve instance required — mirroring the architecture of the Swift
    Package binding at bindings/swift.
  DESC
  s.author         = 'remem-io'
  s.license        = 'Apache-2.0'
  s.homepage       = 'https://github.com/remem-io/remem'
  s.platforms      = {
    :ios => '16.4',
    :tvos => '16.4'
  }
  s.source         = { git: 'https://github.com/remem-io/remem.git' }
  s.static_framework = true

  s.dependency 'ExpoModulesCore'

  # rememhq-core links against libc++ (libremem's C++ sources are compiled
  # into it). The cdylib build resolves this automatically at load time,
  # but the static .a this podspec links against does not.
  s.libraries = 'c++'

  # --- Linking against rememhq-core -----------------------------------
  #
  # There is no published binary/XCFramework yet (see
  # bindings/swift/README.md's "Distributing as an XCFramework" section
  # for the equivalent plan — this binding should eventually share that
  # artifact rather than duplicate it). For now, this links against a
  # local `cargo build` output directory, exactly like bindings/swift's
  # Package.swift does.
  #
  # Build it first, from the repo root:
  #   cargo build --release -p rememhq-core
  #
  # REMEM_LIB_DIR overrides the search path if you've built elsewhere.
  rememhq_core_lib_dir = ENV['REMEM_LIB_DIR'] || File.join(__dir__, '..', '..', '..', 'target', 'release')

  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'SWIFT_OBJC_BRIDGING_HEADER' => '$(PODS_TARGET_SRCROOT)/RememCore/RememCore-Bridging-Header.h',
    'LIBRARY_SEARCH_PATHS' => "$(inherited) #{rememhq_core_lib_dir}",
    'OTHER_LDFLAGS' => '$(inherited) -lrememhq_core',
  }

  s.source_files = "**/*.{h,m,mm,swift,hpp,cpp}"
end
