# AZUL - Desktop GUI framework

<!-- [START badges] -->
[![CI](https://github.com/fschutt/azul/actions/workflows/rust.yml/badge.svg)](https://github.com/fschutt/azul/actions/workflows/rust.yml)
[![Coverage](https://img.shields.io/badge/coverage-report-blue.svg)](https://github.com/fschutt/azul/actions/workflows/rust.yml)
[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust Compiler Version](https://img.shields.io/badge/rustc-1.91%20stable-blue.svg)]()
[![dependency status](https://deps.rs/repo/github/fschutt/azul/status.svg)](https://deps.rs/repo/github/fschutt/azul)
<!-- [END badges] -->

> Azul is a free, functional, reactive GUI framework for Rust, C and C++,
built using the WebRender rendering engine and a CSS / HTML-like document
object model for rapid development of beautiful, native desktop applications

###### [Website](https://azul.rs/) | [Releases](https://azul.rs/ui/releases) | [User guide](https://azul.rs/ui/guide) | [API documentation](https://azul.rs/ui/api)

## Current Status

> [!WARNING]
> **This repository is currently under heavy development. Azul is NOT usable yet.**
> 
> APIs may change frequently and features may be incomplete or unstable.
>
> If you are looking for the old README, see [README-OLD.md](/README-OLD.md)
> 
> The current release is from 2+ years ago, see the [releases page](https://github.com/fschutt/azul/releases).
> 
> Visit https://azul.rs/reftest in order to see the current testing and development
> of the core rendering / HTML layouting engine.

## Building from source

```bash
cargo build -r -p azul-doc
./target/release/azul-doc codegen all                       # generates the API surface of every binding
cargo build -r -p azul-dll --features build-dll             # libazul.{so,dylib,dll} in target/release
```

azul-doc is a multitool that generates all the code *necessary* for having a stable public API that works across multiple languages. 

```bash
# a Rust app against the freshly built library, from the in-repo crate
mkdir -p /path/to/azul/target/azul-lib
cp /path/to/azul/target/release/libazul.dylib /path/to/azul/target/azul-lib/   # .so / .dll elsewhere
export AZ_LINK_PATH=/path/to/azul/target/azul-lib
cargo add azul-dll --rename azul --path /path/to/azul/dll --no-default-features --features link-dynamic
cargo run --release
```

`AZ_LINK_PATH` must not be `target/release` itself: building an app also builds
`azul-dll` without `build-dll`, which writes an empty stub `libazul` into
`target/release` (the build refuses rather than overwrite your library). Copy the
library to a folder of its own first.

### Building the apps (`examples/azul-*`)

The apps link the shared library (`link-dynamic`), so build it first, then each
app in **its own** `cargo build`. Building an app in the same invocation as
`azul-dll --features build-dll` (or `cargo build` for the whole workspace) unifies
the features: the app gets the whole engine compiled in (AzDrive grows to ~75 MB
and no longer uses `libazul`).

```bash
cd /path/to/azul
cargo build -r -p azul-doc && ./target/release/azul-doc codegen all   # after every api.json change
cargo build -r -p azul-dll --features build-dll                         # target/release/libazul.dylib
mkdir -p target/azul-lib && cp target/release/libazul.dylib target/azul-lib/
export AZ_LINK_PATH=$PWD/target/azul-lib

cargo build -r -p AzDrive                       # or AzWriter, AzMail, AzMaps, AzWidgets, ...
cargo build -r -p AzDrive --features fake-providers   # AzDrive for the local test stack (fake payments)

otool -L target/release/AzDrive | grep libazul  # macOS: must list libazul (Linux: ldd ... | grep libazul)
./target/release/AzDrive
```

The app loads the copy of `libazul` the build made for it (its install name is an
absolute path under `target/release/build/azul-dll-*/out/`), so after rebuilding
the library, copy it to `target/azul-lib/` again and rebuild the app (cargo
re-copies it because the library changed). The package names are the app names:
`AzBuilder AzCalculator AzCalendar AzClock AzCode AzContacts AzDashboard AzDrive
AzERP AzInstallManager AzMail AzMaps AzMeet AzMonitor AzMusic AzNews AzNotes AzPaint
AzPasswordManager AzPdf AzPhoto AzPlayer AzReader AzReview AzSetup AzSheets AzShells
AzShow AzTasks AzTerm AzVideoCut AzWidgets AzWriter`.

AzDrive against the local Azlin stack (the azul-apps checkout next to this one):

```bash
cd ../azul-apps && local/fullsim.sh            # ~2 min; prints every address
cd ../azul && . ../azul-apps/local/state/env   # the stack's addresses for the app
./target/release/AzDrive --data-dir /tmp/azdrive-try
# Add drive... > Buy storage > a tier > Card > Buy > card 4242 4242 4242 4242, any future date and CVC
# (4000 0025 0000 3155 asks for 3-D Secure, 4000 0000 0000 0002 is declined)
cd ../azul-apps && local/fullsim.sh --down     # stop it (--wipe also deletes its state)
```
