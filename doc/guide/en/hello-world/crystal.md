---
slug: hello-world/crystal
title: Hello World [Crystal]
language: en
canonical_slug: hello-world/crystal
audience: external
maturity: mature
guide_order: 29
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/crystal/hello-world.cr
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

# Hello World [Crystal]

## Introduction

In order to use `libazul.so` from Crystal, you need the native library and
the generated API bindings which wrap the C API in ordinary Crystal classes.

## Installation

First, you need to install Crystal: `brew install crystal` on macOS, or install it 
from [crystal-lang.org](https://crystal-lang.org/install/).

The preferred way to manage dependencies in Crystal is using `shards`. Azul provides 
a pre-packaged shard containing the generated API bindings.

The preferred way to manage dependencies in Crystal is using `shards`. Azul provides a pre-packaged shard containing the generated API bindings, and a pre-configured `shard.yml` to get you started quickly.

Create a new directory for your project and download the project files:

```sh
mkdir hello-world && cd hello-world
curl -O https://azul.rs/ui/release/$VERSION/shard.yml
curl -O https://azul.rs/ui/release/$VERSION/hello-world.cr
```

Next, download the Azul shard and the native library for your platform into the project root:

```sh
# macOS
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
# Linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
# Windows
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl -O https://azul.rs/ui/release/$VERSION/azul.dll.lib
```

Now, run `shards install` to link the dependency into the `lib/` folder:

```sh
shards install
```

The library path **must be absolute** (`-L$PWD`, not `-L.`): Crystal runs
the linker from its own cache directory, so a relative `-L.` points
there and the link fails. The binary embeds no rpath, which
is why the run step needs `LD_LIBRARY_PATH=.` / `DYLD_LIBRARY_PATH=.`.

### Manual Compilation (Without Shards)

If you prefer a single-file script without a `shard.yml`, you can manually download the single-file wrapper to a `lib/` folder:

```sh
mkdir -p lib && curl -o lib/azul.cr https://azul.rs/ui/release/$VERSION/lib/azul.cr
# Then compile directly using `crystal build`
```

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

Notice the required `--features build-dll`, as this is a flag to "build the DLL, don't link to it". The DLL lands at `target/release/libazul.{so,dylib}` (or `azul.dll`). The bindings end up at `target/codegen/`: `azul.cr` is the one-file binding, and `target/codegen/crystal/` is the same code as a shard (`shard.yml` + `src/`). Copy that directory to `lib/azul/` and `require "azul"` picks it up the same way.

## Simple "Counter" Example

This is the exact program shipped as `examples/crystal/hello-world.cr`:

```crystal
require "azul"

class Counter
  property count : Int32

  def initialize(@count = 5)
  end
end

def on_click(counter : Counter, info : Azul::CallbackInfo) : Azul::Update
  counter.count += 1
  Azul::Update::RefreshDom
end

def layout(counter : Counter, info : Azul::LayoutCallbackInfo) : Azul::Dom
  label = Azul::Dom.p_with_text(counter.count.to_s)
    .with_css("font-size: 32px; margin: 0;")

  button = Azul::Button.new("Increase counter")
    .with_button_type(:primary)
    .with_on_click(counter, ->on_click(Counter, Azul::CallbackInfo))

  Azul::Dom.body
    .with_child(label)
    .with_child(button.dom)
end

window = Azul::WindowCreateOptions.new(->layout(Counter, Azul::LayoutCallbackInfo))
window.window_state.title = "Hello World"
window.window_state.size.dimensions.width = 400
window.window_state.size.dimensions.height = 300

app = Azul::App.new(Counter.new, Azul::AppConfig.new)
app.run(window)
```

1. **The model is a plain class.** `Azul::App.new(Counter.new, ...)`
   hands any Crystal object to the framework. Crystal's garbage collector
   cannot see libazul's heap, so the binding keeps every object and
   callback that libazul references in a table (`Azul::Handles`) and passes
   libazul only a numeric id. The entry is removed when libazul releases
   its last reference.
2. **Callbacks are methods that take the model as its own type.**
   `->on_click(Counter, Azul::CallbackInfo)` passes the top-level method.
   `with_on_click(counter, ...)` upcasts `counter` into a `RefAny`,
   libazul's type-erased, reference-counted handle; on a click, the
   binding's C trampoline downcasts that `RefAny` back to `Counter` (a
   checked cast) and calls `on_click` with it. Handing the same `counter`
   to several callbacks shares one object: a `RefAny` clone is another
   reference to it, not a copy. `layout` gets its `Counter` the same way.
3. **Crystal idioms.** Enum arguments accept symbols
   (`with_button_type(:primary)`) and strings convert to `AzString` on the
   way in.

## Build and run

 From the directory containing your `shard.yml` and the native library, build and run your application:

```sh
# Linux
crystal build hello-world.cr --link-flags "-L$PWD"
LD_LIBRARY_PATH=. ./hello-world

# macOS
crystal build hello-world.cr --link-flags "-L$PWD -framework Foundation -framework AppKit -framework OpenGL -framework CoreGraphics -framework CoreText"
DYLD_LIBRARY_PATH=. ./hello-world

# Windows (PowerShell)
crystal build hello-world.cr --link-flags "$PWD\azul.dll.lib"
.\hello-world.exe
```

On macOS the five `-framework` flags are required: `libazul` calls into
AppKit, OpenGL and CoreText, and a link without them fails with undefined
symbols naming those frameworks. Add `--release` for an optimized build;
the first build of `azul.cr` takes a few seconds, later builds hit
Crystal's cache.

You should see the window pictured on the [hello-world landing page](../hello-world.md). Click the button: the counter should increment, the layout callback then re-runs, and the new value renders.

1. `app.run` opens a native window and runs `layout` once with your `Counter`.
2. The returned `Dom` is styled, laid out, and rendered.
3. The framework then continuously queries whether anything matches the event filter set up in the `Dom`. On click, the framework borrows your model mutably, runs `on_click`, observes the `Azul::Update::RefreshDom` return, and re-invokes the layout callback.
4. The framework determines the diff between the previous frame's `Dom` and the current one, and only re-updates and re-paints the counter, not the entire window.

Congratulations - once you've got the hello-world example running, you've already mastered 80% of the framework. As you might have guessed, more complex UI and styling are only composing more Dom objects together and working with the various event filters. To make this more streamlined, you can now start reading about the [architecture patterns](../architecture.md) or explore what [methods the `Dom` has to offer](../dom.md). See you in the next tutorial!
