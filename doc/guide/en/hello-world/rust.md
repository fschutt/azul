---
slug: hello-world/rust
title: Hello World [Rust]
language: en
canonical_slug: hello-world/rust
audience: external
maturity: mature
guide_order: 11
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - core/src/callbacks.rs
  - core/src/lib.rs
  - dll/src/lib.rs
last_generated_rev: 2660b0c45c9ea401ad6777a203f468755167e62e
generated_at: 2026-09-16T00:00:00Z
default-search-keys:
  - App
  - Dom
  - Css
  - WindowCreateOptions
  - LayoutCallbackInfo
  - CallbackInfo
  - Update
  - RefreshDom
---

# Hello World [Rust]

## Introduction

Azul is a GUI library written in Rust itself. Therefore, it might seem strange for newcomers 
to see the first step being "please download a precompiled `.dll` / `.so` file". However, in 
practice there are very significant benefits once you get over this initial hurdle, that massively 
outweigh the small disadvantage of having to download one `.dll` file:

- Fast recompilation times: only one dependency (the API) instead of hundreds from crates.io
- The library can be compiled as optimized layout code (hot path) while your UI binary can be unoptimized callback code (slow path)
- `/target` directory now only uses a couple MiB instead of GiB of space
- DLLs can integrate with the OS-native package managers such as `apt`, `yum` or `brew` for self-updates
- Multiple Azul applications don't duplicate the library code: one update and all applications are patched
- Faster CI builds: no more recompilation of hundreds of crates

### As a native library

Install it with your package manager (every channel is self-hosted on azul.rs):

```sh
# macOS
brew tap fschutt/azul https://azul.rs/ui/brew.git
brew install fschutt/azul/azul

# Debian / Ubuntu
# (This adds the azul.rs apt repository to your sources.list, so you can receive updates via apt update)
echo "deb [trusted=yes] https://azul.rs/ui/apt stable main" | sudo tee /etc/apt/sources.list.d/azul.list
sudo apt update && sudo apt install azul

# Fedora / RHEL / openSUSE
sudo dnf config-manager --add-repo https://azul.rs/ui/rpm/azul.repo && sudo dnf install azul

# Arch Linux
echo '
[azul]
SigLevel = Optional TrustAll
Server = https://azul.rs/ui/arch/$arch' | sudo tee -a /etc/pacman.conf
sudo pacman -Sy azul

# Alpine Linux
echo "https://azul.rs/ui/alpine" | sudo tee -a /etc/apk/repositories
sudo apk add --allow-untrusted azul

# Windows
choco install libazul --source https://azul.rs/ui/nuget/index.json
# or
scoop bucket add azul https://azul.rs/ui/scoop.git && scoop install azul
```

…or download it next to your project from the
[release page](https://azul.rs/ui/release/$VERSION):

```sh
# Linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so

# macOS M1+
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
# macOS Intel
curl -O https://azul.rs/ui/release/$VERSION/libazul.x86_64.dylib

# Windows
curl.exe -O https://azul.rs/ui/release/$VERSION/azul.dll
curl.exe -O https://azul.rs/ui/release/$VERSION/azul.dll.lib
```

In order for Rust to know where the downloaded dll lives, you need to set the `AZ_LINK_PATH`
variable to the path of the dll. Azul's `build.rs` system is relatively smart and will figure 
out where the other files are that it needs. 

If you want to end up with a "single-binary" build, download the `.a` file and set that as 
your `AZ_LINK_PATH`. 

```sh
export AZ_LINK_PATH=/my/path/to/libazul.so
```

If `AZ_LINK_PATH` is not set, it will try to find the system-installed 
library (i.e. the one installed by apt or brew above) and link against it. So if you installed 
`libazul` via brew or similar, then you don't need to set the environment variable at all - 
only if you used the curl method.

### Installing the API bindings

Now that you have prepared the precompiled library, you still need the generated API bindings. 
Currently, Azul is not on crates.io, but the bindings are available from azul.rs, so you can 
register azul.rs as a "registry" in cargo in your project like this:

```toml
# .cargo/config.toml
[registries]
azul = { index = "sparse+https://azul.rs/ui/cargo/" }
```

```sh
cargo new hello-azul && cd hello-azul
cargo add azul --registry azul
```

Alternatively, add this to your dependencies manually:

```toml
[dependencies]
azul = { version = "$VERSION", registry = "azul" }
```

### Building from source

If you want to build the Azul DLL from source or link it as a "Rust crate" instead 
of using the precompiled .a library, you need to build the azul-dll project (under /dll), 
with `--features build-dll`. 

In order to do this however, you first need to actually generate the bindings again, 
so that the crate even compiles. The bindings are generated from the `api.json` in 
the root folder using the `azul-doc` binary.

```sh
git clone --depth 1 --branch $VERSION https://github.com/fschutt/azul
cargo run --release --manifest-path azul/Cargo.toml -p azul-doc -- codegen all
cargo build --release -p azul-dll --rename azul --path azul/dll --features build-dll
```

## Simple "Counter" Example

The simplest example to showcase Azul's model is only about ~30 lines long:

```rust
use azul::{prelude::*, widgets::Button};

struct DataModel {
    counter: usize,
}

extern "C" 
fn my_layout_func(mut data: RefAny, _: LayoutCallbackInfo) -> Dom {
    let counter = match data.downcast_ref::<DataModel>() {
        Some(d) => format!("{}", d.counter),
        None => return Dom::create_body(),
    };

    let label = Dom::create_p_with_text(counter.as_str())
        .with_css("font-size: 32px; margin: 0;");

    let mut button = Button::create("Increase counter");
    button.set_on_click(data.clone(), my_on_click);
    let button = button.dom().with_css("flex-grow: 1;");

    Dom::create_body().with_child(label).with_child(button)
}

extern "C" 
fn my_on_click(mut data: RefAny, _: CallbackInfo) -> Update {
    let mut data = match data.downcast_mut::<DataModel>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    data.counter += 1;

    Update::RefreshDom
}

fn main() {
    let data = DataModel { counter: 5 };
    let config = AppConfig::create();
    let app = App::create(RefAny::new(data), config);
    let window = WindowCreateOptions::create(my_layout_func);
    app.run(window);
}
```

Notice that `extern "C"` is used because every callback crosses the FFI boundary. 
You use `downcast_ref` (or `downcast_mut`) to recover your concrete struct from 
the type-erased `RefAny`. 

Building the UI is done using primitive node constructors like `Dom::create_p_with_text` 
and `Dom::create_body`, and styled with `with_css("...")`. Finally, `data.clone()` 
just bumps the reference count instead of doing a deep copy.

## Build and run

```sh
cargo run --release
```

You should see the window pictured on the [hello-world landing page](../hello-world.md). 
Click the button: the counter should increment, the layout callback then re-runs, and the 
new value renders.

1. `App::run` opens a native window and runs the layout callback once with your `RefAny`.
2. The returned `Dom` is styled, laid out, and rendered.
3. The framework then continuously queries whether anything matches the event 
   filter set up in the `Dom`. On click, the framework borrows your `RefAny` mutably, 
   runs `my_on_click`, observes the `Update::RefreshDom` return, and re-invokes the 
   layout callback.
4. The framework determines the diff between the previous frame's `Dom` and the 
   current one, and only re-updates and re-paints the counter, not the entire window.

Congratulations - once you've got the hello-world example running, you've already 
mastered 80% of the framework. As you might have guessed, more complex UI and styling 
are only composing more Dom objects together and working with the various event filters. 

You can now start reading about the [architecture patterns](../architecture.md) or 
explore what [methods the `Dom` has to offer](../dom.md). 

See you in the next tutorial!
