# Building and Running an Azul App on the iOS Simulator

This guide will walk you through the process of compiling and running 
your Azul application on the iOS Simulator, directly from the command 
line, without needing to open the Xcode IDE.

The Azul framework is designed to make this process as seamless as 
possible by automating most of the setup.

## Prerequisites

Before you begin, you need a few tools installed on your macOS machine.

### Xcode Command Line Tools

This provides the necessary compilers (Clang), linkers, 
and SDKs for iOS development.

```bash
xcode-select --install
```

### Rust iOS Target

Add the `aarch64-apple-ios-sim` target to your Rust 
toolchain. This target is for building for the iOS 
Simulator on Apple Silicon Macs.

```bash
rustup target add aarch64-apple-ios-sim
```

*(For Intel Macs, you would use `x86_64-apple-ios`)*.

### Code Signing Identity (Optional)

Even for simulator builds, having a code signing identity is 
good practice and will be required for device builds. If you 
have an Apple ID, you can generate a free development certificate.

*   Open Xcode (you may need to install it fully for this step).
*   Go to `Xcode` -> `Settings...` -> `Accounts`.
*   Add your Apple ID.
*   Click `Manage Certificates...` and click the `+` button to create 
    an "Apple Development" certificate.

## Project Setup

Follow these steps to configure your Cargo project.

### `Cargo.toml`

Ensure your `Cargo.toml` specifies a **binary target**. 
This is required for `cargo run` to work.

```toml
[package]
name = "my_app"
version = "0.1.0"
edition = "2021"

[dependencies]
azul = { git = "https://github.com/fschutt/azul", rev = "HASH_HERE" }

[[bin]]
name = "my_app"
path = "src/main.rs"
```

### main.rs

```rust
// src/main.rs
use azul::prelude::*;

extern "C" fn layout(_data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {
    // For simplicity, we create a very basic DOM here.
    Dom::div()
        .with_inline_style("
            width: 100%; 
            height: 100%; 
            background-color: #222222; 
            justify-content: center; 
            align-items: center;
        ")
        .with_child(
            Dom::label("Hello, iOS!")
            .with_inline_style("color: white; font-size: 24px;")
        )
        .style(Css::empty())
}

fn main() {
    let config = AppConfig::default();
    let app = App::new(RefAny::new(()), config);
    let window = WindowCreateOptions::new(layout);
    app.run(window).unwrap();
}
```

### build.rs

Now, create a `build.rs` file in **your project's** root directory. 
This script will run **after `cargo build`** and will automatically 
package your compiled binary into the `.app` bundle required by iOS.

**Copy the following code into `build.rs`:**

```rust
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {

    let target = env::var("TARGET").unwrap_or_default();
    if !target.contains("ios") {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let profile = env::var("PROFILE").unwrap();
    let target_dir = manifest_dir.join("target").join(&target).join(profile);
    let app_name = env::var("CARGO_PKG_NAME").unwrap();
    let executable_path = target_dir.join(&app_name);
    
    let app_bundle_path = target_dir.join(format!("{}.app", &app_name));

    // Create .app directory
    if app_bundle_path.exists() {
        fs::remove_dir_all(&app_bundle_path).unwrap();
    }
    fs::create_dir(&app_bundle_path).unwrap();

    // Copy executable
    fs::copy(&executable_path, app_bundle_path.join(&app_name)).unwrap();

    // Create Info.plist
    let info_plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>{}</string>
    <key>CFBundleIdentifier</key>
    <string>com.example.{}</string>
    <key>CFBundleName</key>
    <string>{}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0</string>
    <key>LSRequiresIPhoneOS</key>
    <true/>
</dict>
</plist>"#,
        &app_name, &app_name, &app_name
    );
    fs::write(app_bundle_path.join("Info.plist"), info_plist_content).unwrap();

    // Code sign the bundle
    let signing_identity = env::var("AZUL_IOS_SIGNING_IDENTITY").unwrap_or_else(|_| "Apple Development".to_string());
    let status = Command::new("codesign")
        .arg("--force")
        .arg("--sign")
        .arg(&signing_identity)
        .arg(&app_bundle_path)
        .status()
        .expect("Failed to execute codesign. Is it in your PATH?");

    if !status.success() {
        panic!("codesign failed. Ensure you have a valid Apple Development identity.");
    }
}
```

## Building and Running on the Simulator

Now you can build and run your app using standard command-line tools.

```bash
# builds in target/aarch64-apple-ios-sim/release/my_app.app
cargo build --release --target aarch64-apple-ios-sim
# list available simulators
xcrun simctl list devices 
# run specific simulator
xcrun simctl boot YOUR_SIMULATOR_UDID
# install your app
xcrun simctl install booted target/aarch64-apple-ios-sim/release/my_app.app
# launch your app
xcrun simctl launch booted com.example.my_app
# shutdown your app
xcrun simctl shutdown YOUR_SIMULATOR_UDID
```

This entire process, from `cargo build` to `xcrun simctl launch`, can be 
wrapped in a simple shell script to make it a one-command operation.
