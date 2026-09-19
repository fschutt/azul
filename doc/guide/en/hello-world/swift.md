---
slug: hello-world/swift
title: Hello World [Swift]
language: en
canonical_slug: hello-world/swift
audience: external
maturity: mature
guide_order: 30
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/swift/hello-world.swift
  - examples/swift/module.modulemap
last_generated_rev: 2660b0c45c9ea401ad6777a203f468755167e62e
generated_at: 2026-09-19T00:00:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - RefAny
  - WindowCreateOptions
  - Update
---

# Hello World [Swift]

## Introduction

The Swift binding is a Swift module named `Azul` (its sources are one file
per API area, in `Azul/`) that wraps the C API in Swift classes and enums.
It sits on top of a Clang module, `CAzul`, which `module.modulemap` builds
from `azul.h`. Your program only does `import Azul`.

You need:

- **Linux / macOS**: Swift 5.10 or newer (Xcode's `swiftc` on macOS).
- **Windows**: Swift **6.x**. Swift 5.10's module map for the Windows C
  runtime fails against current MSVC headers (Visual Studio 2022 17.14,
  MSVC 14.44) with `cyclic dependency in module 'ucrt'`.

Swift gives native structs no guaranteed field order or padding, and has no
way to spell a C tagged union, so the binding cannot redeclare `libazul`'s
types in Swift the way the Zig or Go bindings do. Importing `azul.h` through
the Clang module gives Swift the exact C layout of every type, which is what
makes passing structs by value across the FFI boundary correct.

## Installation

The release bundle `azul-swift-$VERSION.tar.gz` contains the `Azul`
module's sources in `Azul/`, `azul.h`, `module.modulemap` and
`hello-world.swift`. The `Azul` module is built first, then the example
against it:

Linux:

```sh
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
curl -LO https://azul.rs/ui/release/$VERSION/azul-swift-$VERSION.tar.gz
tar xzf azul-swift-$VERSION.tar.gz
swiftc -emit-library -emit-module -module-name Azul -parse-as-library -j8 -I. Azul/*.swift -L. -lazul -o libAzulSwift.so
swiftc -I. hello-world.swift -L. -lAzulSwift -lazul -o hello-world
LD_LIBRARY_PATH=. ./hello-world
```

macOS:

```sh
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
curl -LO https://azul.rs/ui/release/$VERSION/azul-swift-$VERSION.tar.gz
tar xzf azul-swift-$VERSION.tar.gz
swiftc -emit-library -emit-module -module-name Azul -parse-as-library -j8 -I. Azul/*.swift -L. -lazul -o libAzulSwift.dylib
swiftc -I. hello-world.swift -L. -lAzulSwift -lazul \
  -framework Foundation -framework AppKit -framework OpenGL \
  -framework CoreGraphics -framework CoreText -o hello-world
DYLD_LIBRARY_PATH=. ./hello-world
```

Windows:

```sh
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl -O https://azul.rs/ui/release/$VERSION/azul.dll.lib
curl -LO https://azul.rs/ui/release/$VERSION/azul-swift-$VERSION.tar.gz
tar xzf azul-swift-$VERSION.tar.gz
swiftc -emit-library -emit-module -module-name Azul -parse-as-library -j8 -I. Azul/*.swift azul.dll.lib -o AzulSwift.dll
swiftc -I. hello-world.swift AzulSwift.lib azul.dll.lib -o hello-world.exe
hello-world.exe
```

A few details in these commands matter:

- **`-I.`** lets `swiftc` find `module.modulemap` (and through it `azul.h`).
  Without it, the build stops with `no such module 'CAzul'`.
- **The module library is called `AzulSwift`**, not `Azul`. On a
  case-insensitive file system (the macOS and Windows defaults),
  `libAzul.dylib` / `Azul.dll` would be the same file as `libazul.dylib` /
  `azul.dll`.
- **`-j8` compiles the module's files in parallel** (one per API area;
  match it to your core count): about 25 seconds on an 8-core Apple Silicon
  Mac. You only build it once: keep `libAzulSwift` and `Azul.swiftmodule`
  around, and your own program then compiles in seconds. On Windows, run
  these commands in a shell that expands `Azul/*.swift` (Git Bash).
- The binaries embed no rpath, which is why the run step needs
  `LD_LIBRARY_PATH=.` / `DYLD_LIBRARY_PATH=.`.

### Building from source

Only needed if you want to track `master` or patch the library locally:

```sh
# git clone https://github.com/fschutt/azul
# cd myfolder/azul
# generate the bindings from api.json (required)
cargo run -p azul-doc --release -- codegen all
# build the actual DLL with the now-generated .rs C-API bindings
cargo build -p azul-dll --release --features build-dll
```

Notice the required `--features build-dll`, as this is a flag to "build the DLL, don't link to it". The DLL lands at `target/release/libazul.{so,dylib}` (or `azul.dll`). The bindings end up at `target/codegen/`: `azul.h` and `module.modulemap` as above, and `target/codegen/swift/` is the Swift binding as a SwiftPM package (`Package.swift`, and `Sources/Azul/`, which the release ships as `Azul/`).

## Simple "Counter" Example

This is the exact program shipped as `examples/swift/hello-world.swift`:

```swift
import Azul

final class Counter {
    var count = 5
}

func onClick(_ counter: Counter, _ info: CallbackInfo) -> Update {
    counter.count += 1
    return .refreshDom
}

func layout(_ counter: Counter, _ info: LayoutCallbackInfo) -> Dom {
    let label = Dom.pWithText(String(counter.count))
        .withCss("font-size: 32px; margin: 0;")

    let button = Button("Increase counter")
        .withButtonType(.primary)
        .withOnClick(counter, onClick: onClick)

    return Dom.body()
        .withChild(label)
        .withChild(button.dom())
}

let window = WindowCreateOptions(layout)
window.windowState.title = "Hello World"
window.windowState.size.dimensions.width = 400
window.windowState.size.dimensions.height = 300

let app = App(Counter(), AppConfig())
app.run(window)
```

1. **The model is a class.** Callbacks mutate the model in place, so it
   must be a reference type: `withOnClick<T: AnyObject>` only accepts a
   class. The binding retains the object for as long as libazul references
   it (`Unmanaged.passRetained`) and releases it when libazul drops its
   last reference, so there is no manual memory management.
2. **Callbacks are plain functions that take the model as its own type.**
   `withOnClick(counter, onClick: onClick)` upcasts `counter` into a
   `RefAny`, libazul's type-erased, reference-counted handle, which retains
   the object. On a click, the binding's C trampoline downcasts that `RefAny`
   back to `Counter` (a checked cast) and calls `onClick` with it. Handing the
   same `counter` to several callbacks shares one object: a `RefAny` clone is
   another reference to it, not a copy. `layout` gets its `Counter` the same
   way, from the model `App` was created with.
3. **Swift idioms.** Enums use leading-dot syntax (`.primary`,
   `.refreshDom`), Swift `String`s convert to `AzString` on the way in,
   and every `with*` method returns the updated value, so a DOM is one
   expression.

## Build and run

From the directory containing the unpacked bundle and the native
library, once `libAzulSwift` has been built as above:

```sh
# Linux
swiftc -I. hello-world.swift -L. -lAzulSwift -lazul -o hello-world
LD_LIBRARY_PATH=. ./hello-world

# macOS
swiftc -I. hello-world.swift -L. -lAzulSwift -lazul \
  -framework Foundation -framework AppKit -framework OpenGL \
  -framework CoreGraphics -framework CoreText -o hello-world
DYLD_LIBRARY_PATH=. ./hello-world

# Windows
swiftc -I. hello-world.swift AzulSwift.lib azul.dll.lib -o hello-world.exe
hello-world.exe
```

On macOS the five `-framework` flags are required: `libazul` calls into
AppKit, OpenGL and CoreText, and a link without them fails with undefined
symbols naming those frameworks.

You should see the window pictured on the [hello-world landing page](../hello-world.md). Click the button: the counter should increment, the layout callback then re-runs, and the new value renders.

1. `app.run` opens a native window and runs `layout` once with your `Counter`.
2. The returned `Dom` is styled, laid out, and rendered.
3. The framework then continuously queries whether anything matches the event filter set up in the `Dom`. On click, the framework borrows your model mutably, runs `onClick`, observes the `.refreshDom` return, and re-invokes the layout callback.
4. The framework determines the diff between the previous frame's `Dom` and the current one, and only re-updates and re-paints the counter, not the entire window.

Congratulations - once you've got the hello-world example running, you've already mastered 80% of the framework. As you might have guessed, more complex UI and styling are only composing more Dom objects together and working with the various event filters. To make this more streamlined, you can now start reading about the [architecture patterns](../architecture.md) or explore what [methods the `Dom` has to offer](../dom.md). See you in the next tutorial!
