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

### Dynamic "DLL" linking

First, get the prebuilt library. On Debian/Ubuntu and Fedora/RHEL you can
install the `.deb` / `.rpm` package from the GitHub release (installs
`/usr/lib/libazul.so`):

```sh
# linux - Debian / Ubuntu
curl -L -O https://github.com/fschutt/azul/releases/download/0.2.0/azul_0.2.0_amd64.deb
sudo apt install ./azul_0.2.0_amd64.deb

# linux - Fedora / RHEL
curl -L -O https://github.com/fschutt/azul/releases/download/0.2.0/azul-0.2.0-1.x86_64.rpm
sudo dnf install ./azul-0.2.0-1.x86_64.rpm
```

Instead of downloading the `.deb` manually, Debian/Ubuntu users can also
subscribe to the self-hosted apt repository served from azul.rs (it is
unsigned, hence the explicit `[trusted=yes]`):

```sh
echo "deb [trusted=yes] https://azul.rs/ui/apt stable main" | sudo tee /etc/apt/sources.list.d/azul.list
sudo apt update
sudo apt install azul
```

On macOS, a self-hosted Homebrew tap (a real git repository served from
azul.rs) installs `libazul.dylib` into the Homebrew prefix; point
`AZ_LINK_PATH` at `$(brew --prefix)/lib/libazul.dylib`:

```sh
brew tap fschutt/azul https://azul.rs/ui/homebrew-azul.git
brew install fschutt/azul/azul
```

There is currently no Chocolatey package, AUR or Alpine repository. On
Windows, download the prebuilt DLL for your OS from
the [release page](https://azul.rs/ui/release/0.2.0) instead:

```sh
# windows
curl.exe -L -O https://azul.rs/ui/release/0.2.0/azul.dll
# linux
curl -L -O https://azul.rs/ui/release/0.2.0/libazul.so
# macos (Apple Silicon; Intel: libazul.x86_64.dylib)
curl -L -O https://azul.rs/ui/release/0.2.0/libazul.dylib
```

You will then only have one "Rust dependency" when executing `cargo tree`, as the code in the DLL is already precompiled.

If you downloaded the DLL manually (instead of the `.deb` / `.rpm`), export `AZ_LINK_PATH=/path/to/libazul.dylib` (or `.so` / `.dll`):

```sh
# note: lenient DLL path discovery by build.rs
# 
# also accepts the folder path (/my/path/to) and
# auto-discovers .a vs .dylib artifacts (prefers the latter)
# 
# build.rs defaults to system-installed libazul if unset
export AZ_LINK_PATH=/my/path/to/libazul.so
```

Now, you only have to add the main crate (the API bindings) to your project.
Azul is not published on crates.io yet, so the bindings come straight from
the GitHub repository (the package is named `azul-dll`; renaming it to `azul`
gives you the usual `use azul::prelude::*` paths):

```sh
# create new project
cargo new --bin hello-azul
# add the API bindings from the GitHub repo at the 0.2.0 tag
cargo add azul-dll --rename azul \
    --git https://github.com/fschutt/azul --tag 0.2.0 \
    --no-default-features --features link-dynamic
```

The `build.rs` will then automatically configure cargo to link against that library 
(important for shipping to users). Otherwise, it will try to link against the system-installed 
libazul library or panic with a helpful message if your system isn't correctly configured.

Now your application will only be a couple hundred KB large and (re-)compile very fast, 
since rustc only has to recompile your couple of functions, not the azul library code. 
Note that dynamic linking is NOT the default feature set - you have to select it 
with `default-features = false, features = ["link-dynamic"]`:

```toml
[dependencies.azul]
package = "azul-dll"
git = "https://github.com/fschutt/azul"
tag = "0.2.0"
default-features = false
features = ["link-dynamic"]
```

The `build.rs` system is relatively smart: if you have azul installed on your system, 
but `AZ_LINK_PATH` is missing, it will link against the system library. So, installing 
the `.deb` / `.rpm` package (or downloading the DLL and exporting `AZ_LINK_PATH`) 
followed by `cargo run` should work (if not, open a ticket).

### Static "Rust-native" linking

The non-recommended, but still "easiest way" to "simply install" Azul is the 
`link-static` feature, which does a full "build from source" build. It is part 
of the crate's default feature set, so you get it by simply omitting the 
`--no-default-features --features link-dynamic` flags from the `cargo add` above.

```toml
 # build from source (the transitive
 # dependencies come from crates.io)
[dependencies.azul]
package = "azul-dll"
git = "https://github.com/fschutt/azul"
tag = "0.2.0"
features = ["link-static"]
```

This will give you a guaranteed build, but it will download all dependencies from crates.io 
and compile the ~300 dependencies into a bloated ~20MB binary instead of a few hundred KB. 
You'll also have to compile your code in `--release` mode, as usually the performance of the 
framework will be too slow in debug mode. Compiling from source should take about 2 - 4 minutes. 
It is also slow to recompile, as rustc will re-link all dependencies.

The only upside is that your binary is now a self-contained executable without any external 
dependencies. However, you can get the same end-user experience by simply bundling the `.dylib` / `.dll` / `.so`, 
or just downloading the `.a` file instead of the `.dylib` file - then your code will still be 
statically linked in a single binary, but recompile faster.

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

    // Dom::create_p_with_text builds a `<p>` block with the given text node inside.
    // .with_css("...") is the builder counterpart of .set_css(...) - it
    // consumes self and returns a new Dom, so we can chain it inline.
    let label_dom = Dom::create_p_with_text(counter.as_str())
        .with_css("font-size: 50px");

    // We use the "button" widget with its own API
    let mut button = Button::create("Update counter");
    // data.clone() simply bumps the refcount on the refany (thread-safe)
    // and sets what callback handler we will use to mutate this RefAny
    // when the button is actually clicked
    button.set_on_click(data.clone(), my_on_click);

    // Convert the button to a Dom, then override styling via the builder.
    let button_dom = button.dom().with_css("flex-grow: 1");

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
- **`Dom::create_p_with_text`, `Dom::create_div`, `Dom::create_body`** — primitive node constructors. Everything else (buttons, lists, scroll regions) builds on top of them.
- **`with_css("...") / set_css("...")`** — both accept a CSS string. `with_css` is the builder form (consumes `self`, returns a new `Dom`), `set_css` mutates in place. Multi-property strings are valid: `"font-size: 50px; color: white;"`. You can also directly configure `:hover { }`, `:focus { }` and `@media ... { }`, `@os(macos >= sonoma) { }` dynamic queries directly inline — in difference to regular CSS.
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

- [Application Architecture](../architecture.md) — Explains the concepts of architecting a larger Azul application
- [Document Object Model](../dom.md) — The Dom tree - node types, hierarchy, and CSS
- [Hello World [Python]](python.md)
