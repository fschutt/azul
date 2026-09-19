---
slug: hello-world/d
title: Hello World [D]
language: en
canonical_slug: hello-world/d
audience: external
maturity: mature
guide_order: 28
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/d/hello-world.d
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

# Hello World [D]

## Introduction

The D binding is one generated module, `azul.d` (`module azul`), that
wraps the C API in ordinary D structs and classes, with free functions as callbacks. You compile it
together with your program - there is no extra runtime and no code
generator to run on your side.

You need a D compiler with the **2.113 front end or newer**:

- **Linux / Windows**: `dmd` 2.113+ (or LDC).
- **macOS**: **LDC** (`ldc2` 1.43+, `brew install ldc`). `dmd` has no
  Apple Silicon backend: it emits x86_64 objects, which cannot link against
  the arm64 `libazul.dylib` (`symbol(s) not found for architecture x86_64`).
  LDC shares the dmd front end and accepts the same flags.

## Installation

Linux:

```sh
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
curl -O https://azul.rs/ui/release/$VERSION/azul.d
curl -O https://azul.rs/ui/release/$VERSION/hello-world.d
dmd hello-world.d azul.d -L-L. -L-lazul -of=hello-world
LD_LIBRARY_PATH=. ./hello-world
```

macOS:

```sh
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
curl -O https://azul.rs/ui/release/$VERSION/azul.d
curl -O https://azul.rs/ui/release/$VERSION/hello-world.d
ldc2 hello-world.d azul.d -L-L. -L-lazul \
  -L-framework -LFoundation -L-framework -LAppKit -L-framework -LOpenGL \
  -L-framework -LCoreGraphics -L-framework -LCoreText -of=hello-world
DYLD_LIBRARY_PATH=. ./hello-world
```

Windows:

```sh
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl -O https://azul.rs/ui/release/$VERSION/azul.dll.lib
curl -O https://azul.rs/ui/release/$VERSION/azul.d
curl -O https://azul.rs/ui/release/$VERSION/hello-world.d
dmd hello-world.d azul.d -L/LIBPATH:. azul.dll.lib -of=hello-world.exe
hello-world.exe
```

`-L` passes the next flag straight to the linker, so `-L-L. -L-lazul` is
the linker's own `-L. -lazul`. The binary embeds no rpath, which is why
the run step needs `LD_LIBRARY_PATH=.` / `DYLD_LIBRARY_PATH=.`.

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

Notice the required `--features build-dll`, as this is a flag to "build the DLL, don't link to it". The DLL lands at `target/release/libazul.{so,dylib}` (or `azul.dll`). The bindings end up at `target/codegen/`: `azul.d` is the one-file module, and `target/codegen/d/` is the same code as a dub package (`source/azul/*.d`, one module per API area), which compiles faster in parallel.

## Simple "Counter" Example

This is the exact program shipped as `examples/d/hello-world.d`:

```d
module hello_world;

import azul;
import std.conv : to;

final class Counter
{
    int count = 5;
}

Update onClick(Counter counter, CallbackInfo info)
{
    counter.count += 1;
    return Update.refreshDom;
}

Dom layout(Counter counter, LayoutCallbackInfo info)
{
    auto label = Dom.pWithText(counter.count.to!string)
        .withCss("font-size: 32px; margin: 0;");

    auto button = Button("Increase counter")
        .withButtonType(ButtonType.primary)
        .withOnClick(counter, &onClick);

    return Dom.body()
        .withChild(label)
        .withChild(button.dom());
}

void main()
{
    auto window = WindowCreateOptions(&layout);
    window.windowState.title = "Hello World";
    window.windowState.size.dimensions.width = 400;
    window.windowState.size.dimensions.height = 300;

    auto app = App(new Counter, AppConfig());
    app.run(window);
}
```

1. **The model is a plain class.** `App(new Counter, AppConfig())` hands
   any D object to the framework. The binding wraps it in a `RefAny` and
   registers it with `GC.addRoot`: the D garbage collector cannot see
   libazul's heap, so without the root it would free the model while the
   window still uses it. The root is dropped when libazul releases its
   last reference.
2. **Callbacks are free functions.** `onClick` is an ordinary function
   whose parameters are D types (`Counter`, `CallbackInfo`), and you pass
   its address, `&onClick`, exactly like `&layout`. There is no closure
   involved: libazul calls a C trampoline, and an invoker the binding
   generates per callback type and model class
   (`_azulInvoke_ButtonOnClickCallbackType!Counter`) downcasts the `RefAny`
   back to `Counter`, calls your function, and converts the D `Update` it
   returns to the C `AzUpdate`. The model class is checked at compile
   time: `withOnClick(counter, &onClick)` only compiles when `onClick`
   takes a `Counter`.
3. **D strings and builders.** `string` arguments convert to `AzString`
   on the way in, and every `with*` method returns the updated value, so
   a DOM is one expression.

## Build and run

From the directory containing `azul.d`, `hello-world.d` and the native
library:

```sh
# Linux
dmd hello-world.d azul.d -L-L. -L-lazul -of=hello-world
LD_LIBRARY_PATH=. ./hello-world

# macOS
ldc2 hello-world.d azul.d -L-L. -L-lazul \
  -L-framework -LFoundation -L-framework -LAppKit -L-framework -LOpenGL \
  -L-framework -LCoreGraphics -L-framework -LCoreText -of=hello-world
DYLD_LIBRARY_PATH=. ./hello-world

# Windows
dmd hello-world.d azul.d -L/LIBPATH:. azul.dll.lib -of=hello-world.exe
hello-world.exe
```

On macOS the five `-framework` flags are required: `libazul` calls into
AppKit, OpenGL and CoreText, and a link without them fails with undefined
symbols naming those frameworks.

You should see the window pictured on the [hello-world landing page](../hello-world.md). Click the button: the counter should increment, the layout callback then re-runs, and the new value renders.

1. `app.run` opens a native window and runs `layout` once with your `Counter`.
2. The returned `Dom` is styled, laid out, and rendered.
3. The framework then continuously queries whether anything matches the event filter set up in the `Dom`. On click, the framework borrows your model mutably, runs `onClick`, observes the `Update.refreshDom` return, and re-invokes the layout callback.
4. The framework determines the diff between the previous frame's `Dom` and the current one, and only re-updates and re-paints the counter, not the entire window.

Congratulations - once you've got the hello-world example running, you've already mastered 80% of the framework. As you might have guessed, more complex UI and styling are only composing more Dom objects together and working with the various event filters. To make this more streamlined, you can now start reading about the [architecture patterns](../architecture.md) or explore what [methods the `Dom` has to offer](../dom.md). See you in the next tutorial!
