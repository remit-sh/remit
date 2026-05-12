// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "RemitPrompter",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "RemitPrompter",
            path: "Sources/RemitPrompter",
            resources: [.process("Resources")]
        ),
        .testTarget(
            name: "RemitPrompterTests",
            dependencies: ["RemitPrompter"],
            path: "Tests/RemitPrompterTests"
        ),
    ]
)
