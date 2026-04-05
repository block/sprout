// swift-tools-version:5.9
// ABOUTME: Swift Package for Sprout mobile iOS, wrapping the Rust core XCFramework.
// ABOUTME: Depends on SproutCore.xcframework produced by `just mobile-ios`.
import PackageDescription

let package = Package(
    name: "SproutMobile",
    platforms: [
        .iOS(.v16),
    ],
    products: [
        .library(
            name: "SproutMobile",
            targets: ["SproutMobile"]
        ),
    ],
    targets: [
        // Pre-built Rust core. Build with: `just mobile-ios`
        .binaryTarget(
            name: "SproutCoreFFI",
            path: "Frameworks/SproutCore.xcframework"
        ),
        // Swift wrapper around the generated UniFFI bindings.
        .target(
            name: "SproutMobile",
            dependencies: ["SproutCoreFFI"],
            path: "Sources/SproutMobile"
        ),
        .testTarget(
            name: "SproutMobileTests",
            dependencies: ["SproutMobile"],
            path: "Tests/SproutMobileTests"
        ),
    ]
)
