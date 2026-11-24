// swift-tools-version: 6.0
import Foundation
import PackageDescription

let packageRoot = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path

let package = Package(
    name: "EbookManager",
    platforms: [
        .macOS(.v14),
    ],
    products: [
        .executable(name: "EbookManager", targets: ["EbookManager"]),
        .library(name: "EbookShellCore", targets: ["EbookShellCore"]),
        .library(name: "EbookManagerKit", targets: ["EbookManagerKit"]),
    ],
    targets: [
        .target(
            name: "ebook_bridgeFFI",
            path: "Sources/ebook_bridgeFFI",
            publicHeadersPath: "include"
        ),
        .target(
            name: "EbookBridge",
            dependencies: ["ebook_bridgeFFI"],
            path: "Sources/EbookBridge",
            swiftSettings: [.swiftLanguageMode(.v5)],
            linkerSettings: [
                .unsafeFlags(["-L\(packageRoot)/build/rust", "-lebook_bridge"]),
                .linkedLibrary("c++"),
            ]
        ),
        .target(
            name: "EbookShellCore",
            path: "Sources/EbookShellCore",
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        .target(
            name: "EbookManagerKit",
            dependencies: ["EbookBridge", "EbookShellCore"],
            path: "Sources/EbookManagerKit",
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        .executableTarget(
            name: "EbookManager",
            dependencies: ["EbookManagerKit"],
            path: "Sources/EbookManager",
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        .testTarget(
            name: "EbookShellCoreTests",
            dependencies: ["EbookShellCore"],
            path: "Tests/EbookShellCoreTests",
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        .testTarget(
            name: "EbookManagerKitTests",
            dependencies: ["EbookManagerKit", "EbookShellCore"],
            path: "Tests/EbookManagerKitTests",
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
    ]
)
