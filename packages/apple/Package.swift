// swift-tools-version:5.10
import PackageDescription

// What the Mac app and the iOS app share: the bridge to the Rust core, the state, and the views.
// The clients link the core's static library themselves.
let package = Package(
    name: "MotileKit",
    platforms: [.macOS(.v14), .iOS("18.0")],
    products: [
        .library(name: "MotileKit", targets: ["MotileKit"])
    ],
    targets: [
        .target(name: "CMotileCore", path: "Sources/CMotileCore"),
        .target(name: "MotileKit", dependencies: ["CMotileCore"], path: "Sources/MotileKit"),
    ],
    swiftLanguageVersions: [.v5]
)
