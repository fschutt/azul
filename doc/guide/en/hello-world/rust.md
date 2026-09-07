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
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
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

- Fast recompilation times: only one depdency (the API) instead of hundreds from crates.io
- Library can be optimized layout code (hot path) while your UI binary can be unoptimized callback code (slow path)
- `/target` directory now only uses a couple MiB instead of GiB of space
- DLLs can integrate with the OS-native package managers such as `apt`, `yum` or `brew` for self-updates
- Multiple Azul applications don't duplicate the library code: one update and all applications are patched
- Faster CI builds: no more recompilation of hundreds of crates

Additionally, it makes binding to other non-Rust languages also very easy, as Rust isn't the only language on the 
planet (yet). The `azul-doc` codegen system generates the necessary bindings for various languages from the 
"single source of truth" in the `api.json`. It adapts to each languages conventions and generates "wrapepr extras" 
such as integrations with the languages generic, string, vector, optional and error types - so you will, in 
practice, not notice any difference to a regular "crates.io" Rust library. You will only notice that your 
Rust code will recompile much faster and your binary size is now in the kilobyte range.

## Installation

You need a Rust toolchain (1.90+) and the prebuilt `libazul` for your
platform. The Rust API is a *pre-rendered crate* named `azul`: its generated
sources live inside the crate (`src/generated/`), so rust-analyzer resolves
every type and nothing is generated on your machine. The crate links the
prebuilt library — none of azul is compiled in your build — so a hello world
compiles in seconds and only your own code recompiles afterwards.

### 1. The native library

Install it with your package manager (every channel is self-hosted on azul.rs):

```sh
# macOS
brew tap fschutt/azul https://azul.rs/ui/brew.git
brew install fschutt/azul/azul

# Debian / Ubuntu
echo "deb [trusted=yes] https://azul.rs/ui/apt stable main" | sudo tee /etc/apt/sources.list.d/azul.list
sudo apt update && sudo apt install azul

# Fedora / RHEL / openSUSE
sudo dnf config-manager --add-repo https://azul.rs/ui/rpm/azul.repo && sudo dnf install azul

# Arch: the [azul] repository at https://azul.rs/ui/arch  -  Alpine: https://azul.rs/ui/alpine
# Windows: choco install libazul --source https://azul.rs/ui/nuget/index.json
#      or: scoop bucket add azul https://azul.rs/ui/scoop.git && scoop install azul
```

…or download it next to your project from the
[release page](https://azul.rs/ui/release/$VERSION):

```sh
curl -O https://azul.rs/ui/release/$VERSION/libazul.so       # linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib    # macOS (Apple Silicon; Intel: libazul.x86_64.dylib)
curl.exe -O https://azul.rs/ui/release/$VERSION/azul.dll      # windows - plus azul.dll.lib, the import library
curl.exe -O https://azul.rs/ui/release/$VERSION/azul.dll.lib
```

The crate's `build.rs` looks for the library in, in this order: `AZ_LINK_PATH`
(or `AZ_DLL_PATH`; comma-separated directories or library files), the crate
directory and its parent (your project), then the system library directories
(`/opt/homebrew/lib`, `/usr/local/lib`, `/usr/lib`). Chocolatey and Scoop set
`AZ_LINK_PATH` for you. Only an unusual location needs it spelled out:

```sh
export AZ_LINK_PATH=/my/path/to/libazul.so   # a directory works too
```

### 2. The crate, from the azul.rs cargo registry

azul is not on crates.io; azul.rs serves a static
[sparse registry](https://azul.rs/ui/cargo) instead. Register it once (per
project, or in `~/.cargo/config.toml` for every project):

```toml
# .cargo/config.toml
[registries]
azul = { index = "sparse+https://azul.rs/ui/cargo/" }
```

```sh
cargo new hello-azul && cd hello-azul
cargo add azul --registry azul
```

which adds to `Cargo.toml`:

```toml
[dependencies]
azul = { version = "$VERSION", registry = "azul" }
```

Save the counter example below as `src/main.rs` and `cargo run --release`.
`cargo tree` shows exactly one dependency: the code in the DLL is already
compiled.

### 2b. …or the same crate as a download

The crate is also a tarball on the release page. Unpack it into (or next to)
your project and depend on it by path — identical crate, no registry
configuration:

```sh
curl -LO https://azul.rs/ui/release/$VERSION/azul-rust-$VERSION.tar.gz
tar xzf azul-rust-$VERSION.tar.gz
cargo add azul --path ./azul-rust-$VERSION
```

`cargo run --example hello-world` inside `azul-rust-$VERSION/` runs the
counter example from the crate itself.

### Building from source (`link-static`)

The crate in the repository is `azul-dll`. Its default feature set,
`link-static`, compiles all of azul into your binary from source; it needs a
checkout plus the code generator, because that crate's Rust API is generated
rather than committed:

```sh
git clone --depth 1 --branch $VERSION https://github.com/fschutt/azul
cargo run --release --manifest-path azul/Cargo.toml -p azul-doc codegen all
cargo add azul-dll --rename azul --path azul/dll       # link-static is the default
```

`--no-default-features --features link-dynamic` on that crate links the
prebuilt library exactly like the pre-rendered crate does (they share the
same `build.rs` logic).

## Simple "Counter" Example

The simplest example to showcase Azuls model is only about ~30 lines long:

```rust
use azul::prelude::*;
use azul::widgets::Button;

struct DataModel {
    counter: usize,
}

extern "C" fn my_layout_func(mut data: RefAny, _: LayoutCallbackInfo) -> Dom {
    let counter = match data.downcast_ref::<DataModel>() {
        Some(d) => format!("{}", d.counter),
        None => return Dom::create_body(),
    };

    let label = Dom::create_div()
        .with_css("font-size: 32px")
        // A counter display is a LABEL, not prose: a <span> carries no UA
        // paragraph margin (a <p> here grew the line by two font-sizes).
        .with_child(Dom::create_span_with_text(counter.as_str()));

    let mut button = Button::create("Increase counter");
    button.set_on_click(data.clone(), my_on_click);
    let mut button = button.dom();
    button.set_css("flex-grow: 1");

    Dom::create_body().with_child(label).with_child(button)
}

extern "C" fn my_on_click(mut data: RefAny, _: CallbackInfo) -> Update {
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

Five things to notice.

- **`extern "C"`** — every callback crosses the FFI boundary, even in the "Rust-native" case. The signature must be `extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom`, as Azul uses the `C` calling convention instead of the unstable `Rust` calling convention.
- **`downcast_ref::<DataModel>()`** — the runtime cast that recovers your concrete struct from the type-erased `RefAny`. It returns `Option<Ref<DataModel>>` (the `_mut` variant returns `Option<RefMut<DataModel>>`) because at the FFI boundary, the framework cannot statically know the type. The borrow is checked at runtime; if another part of the program already holds a borrow, the cast fails and you must return `Update::DoNothing`.
- **`Dom::create_p_with_text`, `Dom::create_div`, `Dom::create_body`** — primitive node constructors. Everything else (buttons, lists, scroll regions) builds on top of them.
- **`with_css("...") / set_css("...")`** — both accept a CSS string. `with_css` is the builder form (consumes `self`, returns a new `Dom`), `set_css` mutates in place. Multi-property strings are valid: `"font-size: 50px; color: white;"`. You can also directly configure `:hover { }`, `:focus { }` and `@media ... { }`, `@os(macos >= sonoma) { }` dynamic queries directly inline — in difference to regular CSS.
- **`data.clone()`** — `RefAny::clone` bumps the reference count, does not deep-copy your struct. The clone is handed to the button so the click handler can downcast it later.

There are some parts we didn't use such as, which might be interesting to explore next.

- `_: LayoutCallbackInfo`: carries read-only access to the system font cache, image cache, GL context, window size, routing and localization dictionaries
- `WindowCreateOptions` configure window title, size, and decorations (covered in [windowing](../system/windowing.md)).
- `CallbackInfo` has lots of functions with which to navigate, query the DOM, change CSS styles (without needing to rebuild the DOM), query computed layout and styles, etc.

## Build and run

```sh
cargo run --release
```

You should see the window pictured on the [hello-world landing page](..md). Click the button: the counter increments, the layout callback re-runs, and the new value renders.

1. `App::run` opened a native window and ran the layout callback once with your `RefAny`.
2. The returned `Dom` was styled, laid out, and rendered (default: CPU-rendered, because of bad driver issues: usually this is fast enough, can be GPU-rendered if necessary).
3. On click, the button's event filter matched a `MouseUp` inside its hit-test bounds. The framework borrowed your `RefAny` mutably, ran `my_on_click`, observed the `Update::RefreshDom` return, and re-invoked the layout callback.
4. The new `Dom` was diffed against the previous one; only the changed text node was repainted.

## Common errors

- **`downcast_ref` returns `None`** — the `RefAny` is already mutably borrowed elsewhere, or it holds a different type. Return `Dom::create_body()` (or `Update::DoNothing`) and investigate.
- **The window opens blank** — verify your layout callback actually returns a `Dom::create_body()` with children. An empty `Dom` renders to a blank window.
- **The counter does not update** — your click callback returned `Update::DoNothing`. Change to `Update::RefreshDom`.
