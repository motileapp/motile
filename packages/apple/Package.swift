// swift-tools-version:5.10
import PackageDescription

// What the Mac app and the iOS app share: the bridge to the Rust core, the state, and the views.
// The clients link the core's static library themselves. The iOS app draws maps with MapLibre
// Native; the Mac app with MapLibre GL JS, bundled in `Map`.
let package = Package(
    name: "MotileKit",
    platforms: [.macOS(.v14), .iOS("18.0")],
    products: [
        .library(name: "MotileKit", targets: ["MotileKit"])
    ],
    dependencies: [
        .package(url: "https://github.com/maplibre/maplibre-gl-native-distribution", exact: "6.31.0")
    ],
    targets: [
        .target(name: "CMotileCore", path: "Sources/CMotileCore"),
        .target(
            name: "MotileKit",
            dependencies: [
                "CMotileCore",
                .product(name: "MapLibre", package: "maplibre-gl-native-distribution", condition: .when(platforms: [.iOS])),
            ],
            path: "Sources/MotileKit",
            resources: [.copy("Fonts"), .copy("Map")]
        ),
    ],
    swiftLanguageVersions: [.v5]
)
