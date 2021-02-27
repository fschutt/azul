# Azul - Desktop GUI framework

## WARNING: The features advertised in this README may not work yet.

<!-- [START badges] -->
[![Build Status Linux / macOS](https://travis-ci.org/maps4print/azul.svg?branch=master)](https://travis-ci.org/maps4print/azul)
[![Build status Windows](https://ci.appveyor.com/api/projects/status/p487hewqh6bxeucv?svg=true)](https://ci.appveyor.com/project/fschutt/azul)
[![Coverage Status](https://coveralls.io/repos/github/maps4print/azul/badge.svg?branch=master)](https://coveralls.io/github/maps4print/azul?branch=master)
[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE) [![Rust Compiler Version](https://img.shields.io/badge/rustc-1.38%20stable-blue.svg)]()
<!-- [END badges] -->

> Azul is a free, functional, reactive GUI framework for Rust and C++, built using the
Mozilla WebRender rendering engine and a CSS / HTML-like document object model for
rapid development of beautiful desktop applications

###### [Website](https://azul.rs/) | [Tutorial / user guide](https://github.com/maps4print/azul/wiki) | [Video demo](https://www.youtube.com/watch?v=kWL0ehf4wwI) | [Discord Chat](https://discord.gg/nxUmsCG)

## About

Azul is a library for creating graphical user interfaces in Rust. It mixes
paradigms from functional / reactive GUI programming commonly found in games
and game engines with an API suitable for developing desktop applications.
Instead of focusing on an object-oriented approach to GUI programming ("a button
is an object"), it focuses on combining objects by composition ("a button is a function")
and achieves complex layouts by composing widgets into a larger DOM tree.

Azul separates the concerns of business logic / callbacks, data model and UI
rendering / styling by not letting the UI / rendering logic have mutable access
to the application data. In azul, rendering the view is a pure function that maps
your application data to a styled DOM. "Widgets" are just functions that render
a certain state, more complex widgets use function composition.

Since recreating the DOM is expensive (note: "expensive" = 3 milliseconds), azul
caches the DOM object and does NOT recreate it on every frame - only when callbacks
request to recreate it.

Widget-local data that needs to be retained between frames is stored on the DOM
nodes themselves, similar to how the HTML `dataset` property can be used to
store data. The application and widget data is managed using a reference-counted
boxed type (`RefAny`), which can be downcasted to a concrete type if necessary.

## Prerequisites / system dependencies

### Windows / Mac

You do not need to install anything, azul uses the standard system APIs to
render / select fonts.

### Linux

On Linux, you need to install the following packages:

```
clang-cl // needed for compiling the software renderer
linux-libc-dev // needed only for building
libfreetype6-dev // needed to render fonts
libfontconfig1-dev // needed to select system fonts
```

**Arch Linux**: The package for `libfontconfig1-dev` is called `fontconfig`.

NOTE: If you publish an azul-based GUI application, you need to remember to
include these dependencies in your package description, otherwise your
users won't be able to run the application.

## Installation

Due to its large size (and to provide C / C++ interop), azul is built as a dynamic library.
You can download pre-built binaries from [azul.rs/releases](https://azul.rs/releases).

### Using pre-built-binaries

1. Download the library from [azul.rs/releases](https://azul.rs/releases)
2. Set your linker to link against the library
    - Rust: Set `AZUL_INSTALL_DIR` environment variable to the path of the library
    - C++: Copy the `azul.h` on the release page to your project headers and the `azul.dll` to your IDE project.

Note: The API for Rust, C++ and other languages is exactly the same, since the
API is auto-generated. If you want to generate language bindings for your language,
you can generate them using the `public.api.json` file. [See the `/api` folder](https://github.com/maps4print/azul/tree/master/api).

### Building from source

Building the library from source requires clang as well as the prerequisites listed above.

azul requires `clang/cl` as a c compiler, not msvc, since that is required by the swgl

> #### Windows / MSVC
> 1. Install [clang for Windows](https://llvm.org/builds/)
> 2. Make sure you have the [Visual Studio Build Tools](https://visualstudio.microsoft.com/thank-you-downloading-visual-studio/?sku=BuildTools&rel=16) installed
> 3. When starting the Visual Studio Build Tools Installer:
>     - Select "C++ build tools for Windows" (necessary to compile winapi bindings)
>     - additionally select "Clang build tools for Windows"

The `azul-desktop` build script will automatically set `CC=clang-cl` and `CXX=clang-cl`.
If this interferes with your build system, please file an issue.

#### Building from crates.io

By default, you should be able to run

```sh
cargo install --version 1.0.0 azul-dll
```

to compile the DLL from crates.io. The library will be built and installed in
the `$AZUL_INSTALL_DIR` directory, which defaults to
`~/.cargo/lib/azul-dll-0.1.0/target/release/libazul.so`
(or `/azul.dll` on Windows or `/libazul.dylib` on Mac)

#### Building from master

```sh
git clone https://github.com/maps4print/azul
cd azul/azul-dll
cargo build --release --all-features
```

The library will be built in `/azul/target/release/libazul.so`. You will need to
manually set `AZUL_INSTALL_DIR=/azul/target/release/libazul.so`

## Hello World

Note: The widgets are custom to each programming language. All callbacks
have to use `extern "C"` in order to be compatible with the library.
The binary layout of all API types is described in the `public.api.json` file.

![Hello World Application](https://i.imgur.com/KkqB2E5.png)

### Rust

```rust
use azul::prelude::*;
use azul_widgets::{button::Button, label::Label};

struct DataModel {
    counter: usize,
}

// Model -> View
extern "C" fn render_my_view(data: &RefAny, _: LayoutInfo) -> StyledDom {

    let mut result = StyledDom::default();

    let data = match data.downcast_ref::<DataModel>() {
        Some(s) => s,
        None => return result,
    };

    let label = Label::new(format!("{}", data.counter)).dom();
    let button = Button::with_label("Update counter")
        .onmouseup(update_counter, data.clone())
        .dom();

    result
    .append(label)
    .append(button)
}

// View updates model
extern "C" fn update_counter(data: &mut RefAny, event: CallbackInfo) -> UpdateScreen {
    let mut data = data.downcast_mut::<DataModel>().unwrap();
    data.counter += 1;
    UpdateScreen::RegenerateDomForCurrentWindow
}

fn main() {
    let app = App::new(RefAny::new(DataModel { counter: 0 }), AppConfig::default());
    app.run(WindowCreateOptions::new(render_my_view));
}
```

### C++

```cpp
#include "azul.h"
#include "azul-widgets.h"

using namespace azul.prelude;
using azul.widgets.button;
using azul.widgets.label;

struct DataModel {
    counter: uint32_t
}

StyledDom render_my_view(const RefAny& data, LayoutInfo info) {

    const DataModel* data = data.downcast_ref();
    if !(data) {
        return result;
    }

    auto label = Label::new(String::format("{}", &[data.counter])).dom();
    auto button = Button::with_label("Update counter")
       .onmouseup(update_counter, data.clone())
       .dom();

    auto result = StyledDom::default()
        .append(label)
        .append(button);

    return result;
}

UpdateScreen update_counter(RefAny& data, CallbackInfo event) {
    DataModel data = data.downcast_mut().unwrap();
    data.counter += 1;
    return UpdateScreen::RegenerateDomForCurrentWindow;
}

int main() {
    auto app = App::new(RefAny::new(DataModel { .counter = 0 }), AppConfig::default());
    app.run(WindowCreateOptions::new(render_my_view));
}
```

### C

```c
#include "azul.h"
#include "azul-widgets.h"

struct DataModel {
    counter: uint32_t
}

StyledDom render_my_view(const RefAny* data, LayoutInfo info) {

    StyledDom result = az_styled_dom_default();

    DataModel* data = az_refany_downcast_ref(data); // data may be nullptr

    if !(data) {
        return result;
    }

    Label label = az_widget_label_new(az_string_format("{}", &[data.counter]));
    Button button = az_widget_button_with_label("Update counter");
    az_widget_button_set_onmouseup(update_counter, az_refany_shallow_copy(data));

    az_styled_dom_append(az_widget_label_dom(label));
    az_styled_dom_append(az_widget_button_dom(button));

    return result;
}

int main() {
    AzApp app = az_app_new(az_refany_new(DataModel { .counter = 0 }), az_app_config_default());
    az_app_run(app, az_window_create_options_new(render_my_view));
}
```

[Read more about the Hello-World application ...](https://github.com/maps4print/azul/wiki/A-simple-counter)

[Read more about the programming model ...](https://github.com/maps4print/azul/wiki/Getting-Started)

[See the /examples folder for example code in different languages](https://github.com/maps4print/azul/tree/master/examples)

## Performance

A default window, with no fonts or images added takes up roughly 15 - 25MB of RAM.
This usage can go up once you load more images and fonts, since azul has to load
and keep the images in RAM.

The frame time depends on what the renderer needs to do (and whether it uses
hardware acceleration). For a rough estimate, here are the timing results for
a 2012 ThinkPad T420 using embedded Intel graphics on OpenGL 3:

```
Startup time for creating window                                       70.5 ms

Calling the user-defined render_my_view function (50 DOM nodes)...     0.12 ms
Calling the user-defined render_my_view function (4000 DOM nodes)...   3.24 ms

Calculating the full layout once (50 DOM nodes)...                     0.43 ms
Calculating the full layout once (4000 DOM nodes)...                   2.64 ms

Hit testing cursor (50 DOM nodes)...                                   0.05 ms
Hit testing cursor (4000 DOM nodes)...                                 0.11 ms

Recalculating the layout on window resize (50 DOM nodes)...            0.01 ms
Recalculating the layout on window resize (4000 DOM nodes)...          0.23 ms

Rebuilding the display list (50 DOM nodes)...                          0.23 ms
Rebuilding the display list (4000 DOM nodes)...                        0.67 ms

Rendering new display list, 800x600 pixels (50 DOM nodes)...           1.37 ms
Rendering new display list, 3840×2160 pixels (50 DOM nodes)...         1.62 ms
Rendering new display list, 800x600 pixels (4000 DOM nodes)...         1.52 ms
Rendering new display list, 3840×2160 pixels (50 DOM nodes)...         1.95 ms

Animating GPU-accelerated CSS properties (transform / opacity)...      0.04 ms
Scrolling (hit test + re-render) @ 3840×2160...                        0.72 ms

Event handling, callback filtering, etc. (done on every event)         0.05 ms
```

Not every step has to be repeated every frame:

- GPU accelerated animations opacity and transform properties only need a re-render
- Animating non-GPU accelerated properties requires rebuilding the display list and re-rendering
- Changing the window size requires relayouting (re-uses existing allocated nodes),
  display list building and re-rendering, but do NOT require calling the render_my_view function again
- Calling the render_my_view function is done on startup before the window is shown,
  so it only adds to the startup time

Based on these metrics, we can calculate the time needed for various actions:

- GPU animation (transform / opacity) @ 4k with 4000 DOM nodes: 0.04 ms
- Animating `width` @ 4k with 4000 DOM nodes: 0.23 + 0.67 + 1.95ms = 2.55ms
- Animating `width` @ 4k with 50 DOM nodes: 0.23 + 0.67 + 1.95ms = 1.86ms
- Button click requiring full DOM relayout @ 4k with 4000 DOM nodes = 8.07ms

Azuls method for keeping frame time low is "don't render what you don't see".
Usually 4000 DOM nodes are the maximum amount of nodes that a user can view on
the screen (table with 40 columns * 100 rows). Azul can render infinite lists
and tables by rendering only what's on the screen at any given moment.

It is very useful to avoid `UpdateScreen::RegenerateDomForCurrentWindow` to avoid
a full DOM reload. Usually the only time this is necessary is if the UI of the window
changes in a major way.

WebRender also updates GPU texture caches on a background thread, which usually halves
the time needed for rendering.

## Thanks

Several projects have helped severely during the development and should be credited:

- Chris Tollidays [limn](https://github.com/christolliday/limn) framework has helped
  a lot with discovering undocumented parts of WebRender.
- Nicolas Silva for his work on [lyon](https://github.com/nical/lyon) - without this,
  the SVG renderer wouldn't have been possible

## License

This library is MIT-licensed. It was developed by [Maps4Print](https://maps4print.com/),
for quickly producing desktop-based cartography and geospatial mapping GUI applications,
as well as vector or photo editors.