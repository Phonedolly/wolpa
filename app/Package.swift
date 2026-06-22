// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "WolpaApp",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "WolpaApp",
            dependencies: ["WolpaView", "WolpaInput", "WolpaBridge"]
        ),
        .target(
            name: "WolpaView",
            dependencies: ["WolpaBridge"]
        ),
        .target(name: "WolpaInput"),
        .target(
            name: "WolpaBridge"
        ),
        .testTarget(name: "WolpaAppTests", dependencies: ["WolpaApp"]),
    ]
)
