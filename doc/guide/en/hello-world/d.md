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

In order to use `libazul` from D (`dmd` 2.113+ or `ldc2` 1.43+), you need the native 
library and the `azul.d` module, which wraps the underlying C API. 

## Installation

The preferred way to use Azul in D is via the `dub` package manager. Azul 
provides a pre-configured `dub` package containing the API wrappers split 
into parallel-compilable modules (`source/azul/*.d`), which compiles 
significantly faster than the single-file module.

First, create a new `dub` project and download the native `libazul` engine 
for your operating system into the project root:

```sh
mkdir hello-world && cd hello-world
dub init -n .

# macOS
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
# Linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
# Windows
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl -O https://azul.rs/ui/release/$VERSION/azul.dll.lib
```

Next, download and extract the Azul `dub` package:

```sh
curl -LO https://azul.rs/ui/release/$VERSION/azul-d-$VERSION.tar.gz
mkdir azul-d && tar xzf azul-d-$VERSION.tar.gz -C azul-d
```

Update your app's `dub.json` to link the local `azul-d` package and the native library:

```json
{
    "name": "hello-world",
    "dependencies": {
        "azul": { "path": "./azul-d" }
    },
    "lflags-posix": ["-L."],
    "lflags-osx": [
      "-framework", "Foundation", 
      "-framework", "AppKit", 
      "-framework", "OpenGL", 
      "-framework", "CoreGraphics", 
      "-framework", "CoreText"
    ],
    "lflags-windows": ["+azul.dll.lib"]
}
```

Now, simply copy the "Counter" example below into your `source/app.d` file and run the project:

```sh
# macOS / Linux
LD_LIBRARY_PATH=. DYLD_LIBRARY_PATH=. dub run

# Windows
dub run
```

### Manual Compilation

If you prefer to compile manually without a package manager, you 
can download the single-file wrapper (`azul.d`) and compile everything directly:

```sh
curl -O https://azul.rs/ui/release/$VERSION/azul.d
curl -O https://azul.rs/ui/release/$VERSION/hello-world.d

# macOS
ldc2 hello-world.d azul.d -L-L. -L-lazul \
  -L-framework -LFoundation -L-framework -LAppKit -L-framework -LOpenGL \
  -L-framework -LCoreGraphics -L-framework -LCoreText -of=hello-world
DYLD_LIBRARY_PATH=. ./hello-world

# Linux
dmd hello-world.d azul.d -L-L. -L-lazul -of=hello-world
LD_LIBRARY_PATH=. ./hello-world

# Windows
dmd hello-world.d azul.d -L/LIBPATH:. azul.dll.lib -of=hello-world.exe
hello-world.exe
```

Note on the weird `-L-L. -L-lazul`: 

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

Notice the required `--features build-dll`. The DLL lands in 
`target/release/libazul.{so,dylib}` (or `azul.dll`). The bindings are generated 
in `target/codegen/`: `azul.d` is the one-file module, and `target/codegen/d/` 
is the same code as a dub package (one module per API area - which compiles faster 
in parallel).

## Simple "Counter" Example

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

## Internals

Internally, the `new Counter` is tracked via a `GC.addRoot`, so that the
GC doesn't free the object while the layout callback still needs it. Internally, 
the D object is wrapped in an opaque `RefAny`

`onClick` automatically downcasts said `RefAny` again automatically and
returns `Update.doNothing` if the downcast fails (with a log message).
The API auto-converts strings to the expected Rust structs.

## Build and run

If you used the `dub` installation, running your application is as simple as:

```sh
# macOS / Linux
LD_LIBRARY_PATH=. DYLD_LIBRARY_PATH=. dub run

# Windows
dub run
```

If you compiled manually, run the resulting executable ensuring the native 
library is in the library path.

You should see the window pictured on the [hello-world landing page](../hello-world.md). 
Click the button: the counter should increment, the layout callback then re-runs, and the new value renders.

1. `app.run` opens a native window and runs `layout` once with your `Counter`.
2. The returned `Dom` is styled, laid out, and rendered.
3. The framework then continuously queries whether anything matches the event filter set 
   up in the `Dom`. On a click event, the framework borrows your model mutably, runs `onClick`, 
   observes the `Update.refreshDom` return, and re-invokes the layout callback.
4. The framework determines the diff between the previous frame's `Dom` and the current one, 
   and only re-updates and re-paints the counter, not the entire window.

Congratulations - once you've got the hello-world example running, you've already mastered 80% 
of the framework. As you might have guessed, more complex UI and styling are only composing more 
Dom objects together and working with the various event filters. 

You can now start reading about the [architecture patterns](../architecture.md) or explore 
what [methods the `Dom` has to offer](../dom.md). 

See you in the next tutorial!
