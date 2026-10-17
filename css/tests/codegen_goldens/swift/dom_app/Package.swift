// swift-tools-version:5.7
// Copy target/codegen/swift/ to ./azul-swift and libazul next to the binary,
// then: swift build -Xlinker -L. && ./.build/debug/AzulApp
import PackageDescription

let package = Package(
    name: "AzulApp",
    dependencies: [
        .package(path: "azul-swift"),
    ],
    targets: [
        .executableTarget(
            name: "AzulApp",
            dependencies: [.product(name: "Azul", package: "azul-swift")],
            path: "Sources/AzulApp"
        ),
    ]
)
