// swift-tools-version:5.10
import Foundation
import PackageDescription

// The Rust core is a static library built by cargo. `scripts/build-app.sh` builds it and passes
// where it is and what it links against; a plain `swift build` expects it in the repository's
// target/release.
let environment = ProcessInfo.processInfo.environment
let coreFolder = environment["MOTILE_CORE_LIB_DIR"] ?? "\(Context.packageDirectory)/../../target/release"
let coreLinkFlags = (environment["MOTILE_CORE_LINK_FLAGS"] ?? "-framework Security -framework SystemConfiguration -framework CoreFoundation -liconv -lSystem -lc -lm")
    .split(separator: " ")
    .map(String.init)

let package = Package(
    name: "Motile",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "Motile", targets: ["Motile"])
    ],
    targets: [
        .target(name: "CMotileCore", path: "Sources/CMotileCore"),
        .executableTarget(
            name: "Motile",
            dependencies: ["CMotileCore"],
            path: "Sources/Motile",
            swiftSettings: [.swiftLanguageVersion(.v5)],
            linkerSettings: [
                .unsafeFlags(["-L", coreFolder, "-lmotile_core"] + coreLinkFlags)
            ]
        ),
    ]
)
