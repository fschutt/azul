# azul-ios example

Running an azul APP on iOS or iPad requires you to install XCode with a simulator pack first.

```
brew install xcode
brew install xcodegen # installs xcode code generator
rustup target add aarch64-apple-ios x86_64-apple-ios aarch64-apple-ios-sim
cargo install cargo-xcodebuild # installs xcode tools for cargo
cargo xcodebuild boot # list simulator devices and copy the hash

    Simulator device id is required. List of avaliable devices:
    SimulatorDevice { udid: "FC33C0D4-3D49-4EE6-8AAE-F2D9A74BFBD6", name: "iPhone 16 Pro", state: Shutdown }
    SimulatorDevice { udid: "3ABFCBFD-878B-45E4-A372-E37CF4482EAF", name: "iPhone 16 Pro Max", state: Shutdown }
    SimulatorDevice { udid: "81BF050A-6580-48F5-A71F-73D0B271D75E", name: "iPhone 16", state: Shutdown }
    SimulatorDevice { udid: "22097FFB-BD41-437F-8EF2-FDA92FF56B9E", name: "iPhone 16 Plus", state: Shutdown }
    SimulatorDevice { udid: "9E079DF1-5CD2-43AE-909E-98F83D309164", name: "iPhone SE (3rd generation)", state: Shutdown }
    SimulatorDevice { udid: "5DDB35D1-71C6-43CE-AF2E-49680860E1A0", name: "iPad Pro 11-inch (M4)", state: Shutdown }
    SimulatorDevice { udid: "752E4D65-0E95-4962-9220-E8186CA7D7EA", name: "iPad Pro 13-inch (M4)", state: Shutdown }
    SimulatorDevice { udid: "53A54F7C-A2BB-4CBF-A9C9-A8CA2C3369AB", name: "iPad Air 11-inch (M2)", state: Shutdown }
    SimulatorDevice { udid: "5AD77D2D-44D0-41EB-ABC9-9700D8B55764", name: "iPad Air 13-inch (M2)", state: Shutdown }
    SimulatorDevice { udid: "200CAE34-D017-4893-83C7-F4EC5B1713F2", name: "iPad mini (A17 Pro)", state: Shutdown }
    SimulatorDevice { udid: "9C6674A8-6B0B-4A57-B383-53B62F3EE601", name: "iPad (10th generation)", state: Shutdown }

cargo xcodebuild boot 5DDB35D1-71C6-43CE-AF2E-49680860E1A0 # wait for the "iPad Pro 11-inch" simulation to boot

```