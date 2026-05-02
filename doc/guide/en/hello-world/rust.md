---
slug: hello-world/rust
title: Hello World [Rust]
language: en
canonical_slug: hello-world/rust
audience: external
maturity: mature
guide_order: 11
topic_only: false
short_desc: Hello World example in Rust - covers installation, project layout, and simple "counter" app
prerequisites: [hello-world]
tracked_files:
  - api.json
  - core/src/callbacks.rs
  - core/src/lib.rs
  - dll/src/lib.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# Hello World [Rust]

Azul is a GUI library written in Rust, however while working on applications, there are simply some serious problems with Rusts "statically link everything" approach:

- Long (re-)compilation times: GUI frameworks need to be compiled in `--release` mode while the "user code" can run in `debug` mode: if we compile statically, the actual performance would be way too slow, even for development
- Recompilation bloats the `/target` directory 
- Compiled binaries cannot make use of OS-native package managers such as `apt`, `yum` or `brew`: your binary will be self-contained but also at least 25MB large with duplicated code on your users systems

All of this is why Azul is rather built as a "C DLL, that happens to be written in Rust" than a "native Rust library", which makes binding to other non-Rust languages also very easy. The public C API is defined in the `/api.json` file, and the `azul-doc` codegen system generates the necessary bindings for various languages, adapting to each languages conventions, "extras", etc. - so you will not notice any difference to a regular Rust library, only that your Rust code will recompile much faster.

## Installation

### Dynamic "DLL" linking

Even for Rust, which usually links everything statically, the recommended path is that you can download a prebuilt DLL for your OS from the [/releases](/releases) page or use your systems pacakge manager:

```sh
# windows
choco install libazul
# linux - debian-like
apt install libazul
# linux - arch-like
yum install libazul
# macos
brew install libazul
```

```sh
cargo new --bin hello-azul
cargo add azul --version 0.3.0
```

You will now only have one "Rust dependency" when executing `cargo tree`, as the code in the DLL is already precompiled.

Alternatively, download the DLL 'manually' (or in your CI, for faster builds):

```sh
# windows
iex -O https://azul.rs/release/1.0.0-alpha1/azul.dll
# linux
wget -O https://azul.rs/release/1.0.0-alpha1/libazul.so
# macos
wget -O https://azul.rs/release/1.0.0-alpha1/libazul.dylib
```

In the latter case, you then have to export `AZUL_LINK_PATH=/path/to/libazul.dylib` (or `.so` / `.dll`):

```sh
# note: lenient DLL path discovery by build.rs
# 
# also accepts the folder path (/my/path/to) and
# auto-discovers .a vs .dylib artifacts (prefers the latter)
# 
# build.rs defaults to system-installed libazul if unset
export AZUL_LINK_PATH=/my/path/to/libazul.so
```

The `build.rs` will then automatically configure cargo to link against that library (important for shipping to users). Otherwise, it will try to link against the system-installed azul library or panic with a helpful message if your system isn't correctly configured.

Now your application will only be a couple hundred KB large and (re-)compile much faster, since rustc only has to optimized your code, not the azul library code again. This is the default option (enabled with `features = ["link-dynamic"]` by default).

```toml
[dependencies.azul]
version = "0.3"
features = ["link-dynamic"]
```

The `build.rs` system is relatively smart: if you have azul installed on your system, but `AZUL_DLL_PATH` is missing, it will link against the system library. So, a simple `brew install libazul` followed by `cargo run` should work (if not, open a ticket).

### Static "Rust-native" linking

The non-recommended, but still "easiest way" to "simply install" Azul is by enabling the `link-static` feature, which does a full "build from source" build. Because it's _really not recommended_ to do this, you have to enable it with `--features link-static`.

```toml
 # build from source from crates.io
 # not enabled by default: use dynamic linking
 # ideally use systems package manager
[dependencies.azul]
version = "0.3"
features = ["link-static"]
```

This will give you a guaranteed build, but it will download all dependencies from crates.io and compile the ~300 dependencies into a bloated ~20MB binary instead of a few hundred KB. You'll also have to compile your code in `--release` mode, as usually the performance of the framework will be too slow in debug mode. Compiling from source should take about 2 - 4 minutes. It is also slow to recompile, as rustc will re-link all dependencies.

The only upside is that your binary is now a self-contained executable without any external dependencies. However, you can get the same end-user experience by simply bundling the `.dylib` / `.dll` / `.so`, or just downloading the `.a` file instead of the `.dylib` file - then your code will still be statically linked in a single binary, but recompile faster.

## Simple "Counter" Example

The simplest example to showcase Azuls model is only about ~30 lines long:

```rust
// `prelude::*` brings in `App`, `AppConfig`, `Dom`,
// `RefAny`, `Update`, `LayoutCallbackInfo`, 
// `CallbackInfo`, and `WindowCreateOptions`
use azul::prelude::*;
// `widgets::Button` is the built-in button widget
// widgets have to be imported separately, not in prelude
use azul::widgets::Button;

// Define your application data as a plain struct.
struct DataModel {
    // The "single source of truth" for your application state
    counter: usize,
}

// Callback that maps f(DataModel) -> Dom - runs once on 
// startup and when `Update::RefreshDom` is returned by a callback
extern "C" 
fn my_layout_func(data: RefAny, _: LayoutCallbackInfo) -> Dom {
    
    // "RefAny" is a boxed struct that can do a
    // "checked downcast" to your struct
    let counter = match data.downcast_ref::<DataModel>() {
        Some(d) => format!("{}", d.counter),
        None => return Dom::create_body(),
    };

    // Dom::create_text just creates the raw inline text node
    // ("p::text" in CSS) - but we have to wrap it in a <p> block here
    let mut label_dom = Dom::create_p_with_text(counter.as_str());
    label_dom.set_inline_style("font-size: 50px");

    // We use the "button" widget with its own API
    let mut button = Button::create("Update counter");
    // data.clone() simply bumps the refcount on the refany (thread-safe)
    // and sets what callback handler we will use to mutate this RefAny
    // when the button is actually clicked
    button.set_on_click(data.clone(), my_on_click);

    // Then we convert the "button" into its own "dumb Dom object", 
    // so we can override styling
    let mut button_dom = button.dom();
    button_dom.set_inline_style("flex-grow: 1");

    // Final setup and return
    Dom::create_body()
        .with_child(label_dom)
        .with_child(button_dom)
}

extern "C" 
fn my_on_click(mut data: RefAny, _: CallbackInfo) -> Update {

    // Downcast can theoretically fail, but this is not a problem 
    // in practice: worst case clicking button does nothing
    let mut data = match data.downcast_mut::<DataModel>() {
        Some(s) => s,
        None => return Update::DoNothing, // error
    };

    // Here we now mutate the actual data...
    data.counter += 1;

    // And tell Azul to queue a new my_layout_func invocation
    // (dom build -> cascade -> relayout -> display list -> render)
    // 
    // NOTE: Azul aggressively caches resources, diffs the UI after 
    // layout() and reuses layout results. For quick animations, 
    // there are other ways to optimize performance later
    Update::RefreshDom
}

fn main() {
    // Initialize your data model, in whatever way
    let data = DataModel { counter: 0 };
    // AppConfig discovers all the "system config", which you can override,
    // i.e. it will discover "system-native styling", monitors, etc.
    let app_config = AppConfig::create();
    // We can now configure the window(s) to spawn on startup
    // 
    // NOTE: routing, like in a single-page application, is then later 
    // done by swapping the layout callback - this is the "/" default route
    let window_config = WindowCreateOptions::create(my_layout_func);
    // We now "move" the ownership of our data model into the framework
    let app = App::create(RefAny::new(data), app_config);
    // Runs the window - on Win32, this call does not return
    // On other systems it depends on the window_config settings 
    app.run(window_config);
}
```

Five things to notice.

- **`extern "C"`** — every callback crosses the FFI boundary, even in the "Rust-native" case. The signature must be `extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom`, as Azul uses the `C` calling convention instead of the unstable `Rust` calling convention.
- **`downcast_ref::<DataModel>()`** — the runtime cast that recovers your concrete struct from the type-erased `RefAny`. It returns `Option<RefMut<DataModel>>` because at the FFI boundary, the framework cannot statically know the type. The borrow is checked at runtime; if another part of the program already holds a borrow, the cast fails and you must return `Update::DoNothing`.
- **`Dom::create_p_with_text`, `Dom::create_div`, `Dom::create_body`** - primitive node constructors. Everything else (buttons, lists, scroll regions) builds on top of them.
- **`set_inline_style("...")`** — accepts a CSS string. Multi-property strings are valid: `"font-size: 50px; color: white;"`. Also you can directly configure `:hover { }`, `:focus { }` and `@media ... { }`, `@os macos >= sonoma { }` dynamic queries directly inline - in difference to regular CSS.
- **`data.clone()`** — `RefAny::clone` bumps the reference count, does not deep-copy your struct. The clone is handed to the button so the click handler can downcast it later.

There are some parts we didn't use such as, which might be interesting to explore next.

- `_: LayoutCallbackInfo`: carries read-only access to the system font cache, image cache, GL context, window size, routing and localization dictionaries
- `WindowCreateOptions` configure window title, size, and decorations (covered in [windowing](../windowing.md)).
- `CallbackInfo` has lots of functions with which to navigate, query the DOM, change CSS styles (without needing to rebuild the DOM), query computed layout and styles, etc.

## Build and run

```sh
cargo run --release
```

You should see the window pictured on the [hello-world landing page](../hello-world.md). Click the button: the counter increments, the layout callback re-runs, and the new value renders.

1. `App::run` opened a native window and ran the layout callback once with your `RefAny`.
2. The returned `Dom` was styled, laid out, and rendered (default: CPU-rendered, because of bad driver issues: usually this is fast enough, can be GPU-rendered if necessary).
3. On click, the button's event filter matched a `MouseUp` inside its hit-test bounds. The framework borrowed your `RefAny` mutably, ran `my_on_click`, observed the `Update::RefreshDom` return, and re-invoked the layout callback.
4. The new `Dom` was diffed against the previous one; only the changed text node was repainted.

## Common errors

- **`downcast_ref` returns `None`** — the `RefAny` is already mutably borrowed elsewhere, or it holds a different type. Return `Dom::create_body()` (or `Update::DoNothing`) and investigate.
- **The window opens blank** — verify your layout callback actually returns a `Dom::create_body()` with children. An empty `Dom` renders to a blank window.
- **The counter does not update** — your click callback returned `Update::DoNothing`. Change to `Update::RefreshDom`.

## Coming Up Next

- [DOM and Callbacks](../dom.md) — explains how to build richer trees, accessibility
- [Events and Input](../events.md) — beyond `MouseUp`: hover, focus, keyboard.
