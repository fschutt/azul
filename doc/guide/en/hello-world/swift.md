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

In order to use `libazul` from Swift (5.10+ on Linux / macOS, 6+ on Windows), you need the native library and the generated API bindings.

## Installation

Download the release bundle, which contains the Swift bindings (`Azul/`), the C interop map (`module.modulemap`), and the example script (`hello-world.swift`).

```sh
# macOS
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
curl -LO https://azul.rs/ui/release/$VERSION/azul-swift-$VERSION.tar.gz
tar xzf azul-swift-$VERSION.tar.gz

# Linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
curl -LO https://azul.rs/ui/release/$VERSION/azul-swift-$VERSION.tar.gz
tar xzf azul-swift-$VERSION.tar.gz

# Windows
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl -O https://azul.rs/ui/release/$VERSION/azul.dll.lib
curl -LO https://azul.rs/ui/release/$VERSION/azul-swift-$VERSION.tar.gz
tar xzf azul-swift-$VERSION.tar.gz
```

### Building from source

Only needed if you want to track `master` or patch the library locally:

```sh
# git clone https://github.com/fschutt/azul
# cd myfolder/azul
cargo run -p azul-doc --release -- codegen all
cargo build -p azul-dll --release --features build-dll
```

The DLL lands at `target/release/libazul.{so,dylib}` (or `azul.dll`). The bindings end up in `target/codegen/swift/`.

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

1. **The model is a class.** Callbacks mutate the model in place, so it must be a reference type. The binding retains the object automatically for as long as the framework references it.
2. **Callbacks are plain functions.** They take your specific model type (`Counter`) directly. The framework handles type-erasure and downcasting behind the scenes. 
3. **Swift idioms.** Enums use leading-dot syntax (`.primary`, `.refreshDom`), Swift `String`s convert natively, and every `with*` method returns the updated value so a DOM can be built in one expression.

## Build and run

Compile the `Azul` module first, then compile your application against it. 

*(Note: The module library is named `AzulSwift` to prevent filename collisions with `libazul.so/dylib` on case-insensitive filesystems).*

```sh
# Linux
swiftc -emit-library -emit-module -module-name Azul -parse-as-library -j8 -I. Azul/*.swift -L. -lazul -o libAzulSwift.so
swiftc -I. hello-world.swift -L. -lAzulSwift -lazul -o hello-world
LD_LIBRARY_PATH=. ./hello-world

# macOS
swiftc -emit-library -emit-module -module-name Azul -parse-as-library -j8 -I. Azul/*.swift -L. -lazul -o libAzulSwift.dylib
swiftc -I. hello-world.swift -L. -lAzulSwift -lazul \
  -framework Foundation -framework AppKit -framework OpenGL \
  -framework CoreGraphics -framework CoreText -o hello-world
DYLD_LIBRARY_PATH=. ./hello-world

# Windows (Git Bash)
swiftc -emit-library -emit-module -module-name Azul -parse-as-library -j8 -I. Azul/*.swift azul.dll.lib -o AzulSwift.dll
swiftc -I. hello-world.swift AzulSwift.lib azul.dll.lib -o hello-world.exe
./hello-world.exe
```

You should see the window pictured on the [hello-world landing page](../hello-world.md). Click the button: the counter should increment, the layout callback then re-runs, and the new value renders.

1. `app.run` opens a native window and runs `layout` once with your `Counter`.
2. The returned `Dom` is styled, laid out, and rendered.
3. The framework continuously queries whether anything matches the event filters. On click, it runs `onClick`, observes `.refreshDom`, and re-invokes the layout callback.
4. The framework computes the diff between the old and new `Dom`, and only repaints the counter.

Congratulations! You can now start reading about the [architecture patterns](../architecture.md) or explore the [methods `Dom` has to offer](../dom.md). See you in the next tutorial!
