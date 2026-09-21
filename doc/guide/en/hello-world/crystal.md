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

Dependencies in Crystal are managed with `shards`, and Azul publishes the
generated binding as a shard in a git repository - the same way the Homebrew
tap and the Scoop bucket are served. `shards install` clones it into `lib/`,
which is where `require "azul"` looks.

Create a directory for your project and download the manifest and the
example:

```sh
mkdir hello-world && cd hello-world
curl -O https://azul.rs/ui/release/$VERSION/shard.yml
curl -O https://azul.rs/ui/release/$VERSION/hello-world.cr
```

The manifest declares `azul` as a git dependency on
`https://azul.rs/ui/crystal.git`. The shard is the *binding* only - the native
library is a separate download, because it is platform-specific:

```sh
# macOS
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
# Linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
# Windows
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl -O https://azul.rs/ui/release/$VERSION/azul.dll.lib
```

Now `shards install` clones the shard into `lib/azul/`, and `crystal build`
compiles against it:

```sh
shards install

# macOS
crystal build hello-world.cr --link-flags "-L$PWD -framework Foundation -framework AppKit -framework OpenGL -framework CoreGraphics -framework CoreText"
# Linux
crystal build hello-world.cr --link-flags "-L$PWD"
# Windows
crystal build hello-world.cr --link-flags "azul.dll.lib"

DYLD_LIBRARY_PATH=. ./hello-world   # macOS
LD_LIBRARY_PATH=. ./hello-world     # Linux
```

The library path must be absolute (`-L$PWD`, not `-L.`): Crystal runs
the linker from its own cache directory, so a relative `-L.` points
there and the link fails. The binary embeds no rpath, which
is why the run step needs `LD_LIBRARY_PATH=.` / `DYLD_LIBRARY_PATH=.`.

### Manual Compilation

If you prefer a single-file script without a `shard.yml`, you can manually download 
the single-file wrapper to a `lib/` folder:

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

Notice the required `--features build-dll`. The DLL lands in
`target/release/libazul.{so,dylib}` (or `azul.dll`). The bindings end up in
the `target/codegen/` folder: `azul.cr` is the one-file binding, and 
`target/codegen/crystal/` is the same code as a shard (`shard.yml` + `src/`). 
Copy that directory to `lib/azul/` and `require "azul"` picks it up the same way.

## Simple "Counter" Example

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

## Binding Internals

Internally, Azul wraps the object returned by `Counter.new` in a `Azul::Handles`
table and only stores that numeric ID, then uses it in the automatic downcasting
(before invoking the `def layout` with it). This is relatively standard practice 
for GC languages, so that the garbage collector doesn't delete the object while 
it's still needed for the next `layout()` call.

Additionally, callbacks are methods that take the model as its own type, such as
`->on_click(Counter, Azul::CallbackInfo)` - this way we know exactly what we need to
downcast to again when invoking the callback.

Handing the same `counter` to several callbacks shares one object: a `RefAny` clone 
is another reference to it, not a copy. `layout` gets its `Counter` the same way.

The bindings allow you to use native Crystal symbols as enum arguments
(`with_button_type(:primary)`) and strings convert to `AzString` automatically.

## Build and run

 From the directory containing your `shard.yml` and the native library, 
 build and run your application:

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

On macOS the five `-framework` flags are required. Add `--release` for 
an optimized build; the first build of `azul.cr` takes a few seconds.

You should see the window pictured on the [hello-world landing page](../hello-world.md). 
Click the button: the counter should increment, the layout callback then re-runs, and the new value renders.

1. `app.run` opens a native window and runs `layout` once with your `Counter`.
2. The returned `Dom` is styled, laid out, and rendered.
3. The framework then continuously queries whether anything matches the event filter 
   set up in the `Dom`. On click, the framework borrows your model mutably, runs `on_click`, 
   observes the `Azul::Update::RefreshDom` return, and re-invokes the layout callback.
4. The framework determines the diff between the previous frame's `Dom` and the current one, 
   and only re-updates and re-paints the counter, not the entire window.

Congratulations - once you've got the hello-world example running, you've already mastered 
80% of the framework. As you might have guessed, more complex UI and styling are only composing 
more Dom objects together and working with the various event filters. 

You can now start reading about the [architecture patterns](../architecture.md) or explore 
what [methods the `Dom` has to offer](../dom.md). 

See you in the next tutorial!
