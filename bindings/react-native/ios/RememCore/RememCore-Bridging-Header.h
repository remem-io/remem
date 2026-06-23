// RememCore-Bridging-Header.h
//
// Exposes rememhq-core's C ABI (rememhq.h) to the Swift code in this pod
// target. Referenced via SWIFT_OBJC_BRIDGING_HEADER in Remem.podspec.
//
// Unlike bindings/swift (a Swift Package, which uses a CRemem module
// target + module map), CocoaPods pod targets conventionally use a
// bridging header for plain C/Objective-C interop within the same
// target — there's no need for a separate Clang module here since
// nothing outside this pod needs to import this header directly.

#import "rememhq.h"
