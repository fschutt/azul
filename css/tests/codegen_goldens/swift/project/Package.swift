// swift-tools-version:5.7
// Copy target/codegen/swift/ to ./azul-swift and libazul next to the binary,
// then: swift build -Xlinker -L. && ./.build/debug/AzulStyles
import PackageDescription

let package = Package(
    name: "AzulStyles",
    dependencies: [
        .package(path: "azul-swift"),
    ],
    targets: [
        .executableTarget(
            name: "AzulStyles",
            dependencies: [.product(name: "Azul", package: "azul-swift")],
            path: "Sources/AzulStyles"
        ),
    ]
)
