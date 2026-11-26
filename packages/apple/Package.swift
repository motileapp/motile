// swift-tools-version:5.10
import Foundation
import PackageDescription

// What the Mac app and the iOS app share: the bridge to the Rust core, the state, and the views.
// The clients link the core's static library themselves; the tests link the one cargo builds for
// their platform, or the one in MOTILE_CORE_LIB_DIR. The iOS app draws maps with MapLibre Native;
// the Mac app with MapLibre GL JS, bundled in `Map`. MotileShared is what the iOS app's extensions
// share with it, without the core.
let environment = ProcessInfo.processInfo.environment
let repository = "\(Context.packageDirectory)/../.."

func core(_ triple: String, flags: String) -> [String] {
    ["-L", environment["MOTILE_CORE_LIB_DIR"] ?? "\(repository)/target/\(triple)release", "-lmotile_core"] + flags.split(separator: " ").map(String.init)
}

let package = Package(
    name: "MotileKit",
    platforms: [.macOS(.v14), .iOS("18.0")],
    products: [
        .library(name: "MotileKit", targets: ["MotileKit"]),
        .library(name: "MotileShared", targets: ["MotileShared"]),
    ],
    dependencies: [
        .package(url: "https://github.com/maplibre/maplibre-gl-native-distribution", exact: "6.31.0"),
        .package(url: "https://github.com/pointfreeco/swift-snapshot-testing", from: "1.19.0"),
    ],
    targets: [
        .target(name: "CMotileCore", path: "Sources/CMotileCore"),
        .target(name: "MotileShared", path: "Sources/MotileShared"),
        .target(
            name: "MotileKit",
            dependencies: [
                "CMotileCore",
                "MotileShared",
                .product(name: "MapLibre", package: "maplibre-gl-native-distribution", condition: .when(platforms: [.iOS])),
            ],
            path: "Sources/MotileKit",
            resources: [.copy("Fonts"), .copy("Map")]
        ),
        .testTarget(
            name: "MotileKitTests",
            dependencies: ["MotileKit", .product(name: "SnapshotTesting", package: "swift-snapshot-testing")],
            path: "Tests/MotileKitTests",
            linkerSettings: [
                .unsafeFlags(
                    core("", flags: "-framework Security -framework SystemConfiguration -framework CoreFoundation -liconv"),
                    .when(platforms: [.macOS])),
                .unsafeFlags(
                    core("aarch64-apple-ios-sim/", flags: "-framework Network -framework SystemConfiguration -framework Security -liconv"),
                    .when(platforms: [.iOS])),
            ]
        ),
        .testTarget(name: "MotileSharedTests", dependencies: ["MotileShared"], path: "Tests/MotileSharedTests", resources: [.copy("Fixtures")]),
    ],
    swiftLanguageVersions: [.v5]
)
