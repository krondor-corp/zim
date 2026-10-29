// swift-tools-version:5.3
import PackageDescription

let package = Package(
    name: "tauri-plugin-zim-web-auth",
    platforms: [
        .macOS(.v10_13),
        .iOS(.v13),
    ],
    products: [
        .library(
            name: "tauri-plugin-zim-web-auth",
            type: .static,
            targets: ["tauri-plugin-zim-web-auth"]),
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api")
    ],
    targets: [
        .target(
            name: "tauri-plugin-zim-web-auth",
            dependencies: [
                .byName(name: "Tauri")
            ],
            path: "Sources")
    ]
)
