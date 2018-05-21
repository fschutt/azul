# azul

[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Build Status Linux / macOS](https://travis-ci.org/maps4print/azul.svg?branch=master)](https://travis-ci.org/maps4print/azul)
[![Build status Windows](https://ci.appveyor.com/api/projects/status/p487hewqh6bxeucv?svg=true)](https://ci.appveyor.com/project/fschutt/azul)
[![coveralls](https://coveralls.io/repos/github/maps4print/azul/badge.svg?branch=master)](https://coveralls.io/github/maps4print/azul?branch=master)
[![codecov](https://codecov.io/gh/maps4print/azul/branch/master/graph/badge.svg)](https://codecov.io/gh/maps4print/azul)
[![Rust Compiler Version](https://img.shields.io/badge/rustc-1.26%20stable-blue.svg)]()

azul is a cross-platform, stylable GUI framework using Mozillas `webrender` engine for rendering
and a CSS / DOM model for layout and rendering

[Crates.io](https://crates.io/crates/azul) | [Library documentation](https://docs.rs/azul) | [User guide](http://azul.rs/)

## Design

azul is a library designed from the experience gathered during working with other
GUI toolkits. azul is very different from (QT / GTK / FLTK / etc.) in the following regards:

- GUIs are seen as a "view" into your applications data, they are not "objects" like
  in any other toolkit. There is no `button.setActive(true)` for example, as that would
  introduce stateful design.
- Widgets types are simply enums that "serialize" themselves into a DOM tree.
- The DOM is immutable and gets re-generated every frame. This makes testing
  and debugging very easy, since if you give the `get_dom()` function a
  specific data model, you always get the same DOM back (`get_dom()` is a pure function).
  This comes at a slight performance cost, however in practice the cost isn't too
  high and it makes the seperation of application data and GUI data very clean.
- The layout model closely follows the CSS flexbox model. The default for CSS is
  `display:flex` instead of `display:static` (keep that in mind). Some semantics
  of CSS are not the same, especially the `image` and `vertical-align` properties.
  However, most attributes work in azul, same as they do in CSS, i.e. `color`,
  `linear-gradient`, etc.
- azul trades a slight bit of performance for better usability. azul is not meant
  for game UIs, it is currently too slow for that (currently using 2 - 4 ms per frame)
- azul does not have any asyncronous callbacks - you can implement them manually by
  using threads, but we will wait until the Rust compiler stabilizes async / await
  later this year (should be stabilized until Dec 2018).
- azul links everything statically, including freetype and

## Data model / Reactive programming

To understand how to efficiently program with azul, you have to understand its
programming / memory model. One image says more than 1000 words:

![Azul callback model](https://raw.githubusercontent.com/maps4print/azul/master/doc/azul_callback_model.png)

This creates a very simple programming flow:

```rust
// Your data model
struct DataModel {
    /* store anything you want here that is relevant to the application */
}

// Data model -> DOM
impl LayoutScreen for DataModel {
    fn get_dom(&self, _window_id: WindowId) -> Dom<Self> {
        /* DataModel is read-only here, "serialize" from the data model into a UI */
        Dom::new(NodeType::Button { text: hello, .. })
            .with_event(On::MouseDown, Callback::Sync(my_button_was_clicked))
    }
}

// Callback updates data model, when the button is clicked
fn my_button_was_clicked(_app_state: &mut AppState<MyAppData>) -> UpdateScreen {
    println!("Button clicked!");
    // performance optimization, tell azul that this function doesn't change the UI
    // azul will still redraw when the window is resized / CSS events changed
    // but by default, azul only redraws when it's absolutely necessary.
    UpdateScreen::DontRedraw
}

fn main() {
    let mut app = App::new(DataModel { });
    app.create_window(WindowCreateOptions::default(), Css::native()).unwrap();
    app.run();
}
```

This makes it easy to compose the UI from a set of functions, where each function
creates a sub-DOM that can be composed into a larger UI:

```rust
impl LayoutScreen for DataModel {
    fn get_dom(&self, _window_id: WindowId) -> Dom<Self> {
        let mut dom = Dom::new();
        if !self.is_email_sent {
            dom.add_child(email_recipients_list(&self.names));
            dom.add_child(email_send_button());
        } else {
            dom.add_child(no_email_label());
        }
        dom
    }
}

fn email_recipients_list(names: &[String]) -> Dom<DataModel> {
    let mut names_list = Dom::new(NodeType::Div);
    for name in names {
        names_list.add_child(Dom::new(NodeType::Label {
            text: name,
        }));
    }
    names_list
}

fn email_send_button() -> Dom<DataModel> {
    Dom::new(NodeType::Button { text: hello, .. })
        .with_id("email-send-button")
        .with_event(On::MouseDown, Callback::Sync(my_button_was_clicked))
}

fn no_email_label() {
    Dom::new(NodeType::Button { text: "No email to send!", .. })
        .with_id("email-done-label")
}

fn send_email(app_state: &mut AppState<DataModel>, _window_id: WindowId) -> UpdateScreen {
    app_state.data.is_email_sent = false;
    // trigger a redraw, so the list gets removed from the screen
    // and the "you're done" message is displayed
    UpdateScreen::Redraw
}
```

And this is why azul doesn't really have a large API like other frameworks -
that's really all there is to it! Didn't I say it was simple to learn?

The benefit of this is that it's very simple to test:

```rust
#[test]
fn test_it_should_send_the_email() {
    let mut initial_state = AppState::new(DataModel { is_email_sent: false });
    send_email(&mut initial_state, WindowId::new(0));
    assert_eq!(initial_state.is_email_sent, true);
}
```

However, this model gets a bit tricky when you want to know about additional
information in the callback (such as determining which email recipient of the
list was clicked on).

// TODO: explain how to send hit test IDs in the callback

## Updating window properties

You may have noticed that the callback takes in a `AppState<DataModel>`, not
the `DataModel` directly. This is because you can change the window settings, for
example the title of the window:

```rust
fn callback(app_state: &mut AppState<DataModel>, window_id: WindowId) -> UpdateScreen {
    app_state.windows[window_id].window.title = "Hello";
    app_state.windows[window_id].window.menu += "&Application > &Quit\tAlt+F4";
}
```

Note how there isn't any `.get_title()` or `.set_title()`. Simply setting the title
is enough to invoke the (stateful) Win32 / X11 / Wayland / Cocoa functions for setting
the window title.

## Working with blocking IO

Bloking IO is when you have to wait for something to complete, a website returning
HTMl / JSON. Azul has to continouusly poll if the execution has finished.

For this, azul has a mechanism called a "Task" (similar to C#). A Task starts a
background thread and azul registers it and looks if the thread has finished yet.
Usually you lock the data model when the task is done, i.e. when you've finished loading
the file / website, etc.

```rust

struct DataModel {
    website_data: Option<String>,
}

impl LayoutScreen {
    /// Note: `get_dom` is called in a thread-safe way by azul.
    fn get_dom(&self, _window_id: WindowId) -> Dom<Self> {
        let mut dom = Dom::new();
        match self.website_data {
            Some(data) => dom.append_child(Dom::new(NodeType::Label { text: data.clone() })),
            None => dom.append_child(
                Dom::new(NodeType::Button { text: "Download the website", .. })
                    .with_event(On::Click, Callback::Async(start_download))),
        }
        dom
    }
}

// Note: push_background_task are only implemented on Arc<Mutex<T>>, not T itself.
fn start_download(app_state: &mut Arc<Mutex<AppState<DataModel>>>, _window_id: WindowId) -> UpdateScreen {
    // background_fns creates a background thread that clones the app_state Arc,
    // waits until the thread has completed and then calls `get_dom()` on completion, to update the UI
    app_state.push_background_task(Task(download_website));
    UpdateScreen::DontRedraw
}

// Note: The `_drop` is necessary so that azul can tell that the thread has finished executing.
fn download_website(app_state: Arc<Mutex<AppState<DataModel>>>, _drop: Arc<()>) {
    // simulate slow, blocking IO
    ::std::thread::sleep(::std::time::Duration::from_secs(5));
    // only lock the Arc when done with the work
    let app_state = app_state.lock().unwrap();
    app_state.data.website_data = Some("<html><h1>Hello</h1></html>".into());
}
```

Note that there is no "wait". If you call `app_state.lock()` in the `download_website` function,
it will block the main thread, so only call it once you are done with the blocking IO.

These concepts currently start full, OS-level threads. However, generally in
desktop applications, you don't start 10k tasks at once, maybe 4 - 5 max. This concept
will be replaced by async / await syntax, until then it uses the OS threads.

## Deamons

Sometimes you want to run functions independent of the user interacting with the application.
Example: you want to update a progress bar to how what percentage of a file has loaded. Or
you want to start a timer. For this, azul has "deamons" or "polling functions", that run
continouusly in the background, until they stop.

```rust
use std::time::Duration;

struct DataModel {
    // technically you'd only need to store the Instant of the start,
    // but this is just to demonstrate how deamons work
    stopwatch: Option<(Instant, Duration)>,
}

impl LayoutScreen {
    // pseudocode, you can imagine what display_stop_watch, create_stop_timer_btn
    // and create_start_timer_button do
    fn get_dom(&self, _window_id: WindowId) -> Dom<Self> {
        let mut dom = Dom::new();
        match stopwatch {
            Some(_, current_duration) => {
                dom.append_child(display_stop_watch(current_duration));
                dom.append_child(
                    create_stop_timer_btn()
                    .with_callback(On::MouseDown, Callback::Sync(stop_timer))
                );
            },
            None => {
                dom.append_child(
                    create_start_timer_button()
                    .with_callback(On::MouseDown, Callback::Sync(start_timer))
                )
            }
        }
        dom
    }

    fn start_timer(app_state: &mut AppState<DataModel>>>, _window_id: WindowId) -> UpdateScreen {
        app_state.stopwatch = Some(Instant::now(), Duration::from_secs(0));
        // Deamons are identified by ID, to allow to run ex. multiple timers at once
        app_state.push_deamon("timer_1", Callback::Sync(update_timer));
        UpdateScreen::Redraw
    }

    fn update_timer(app_state: &mut AppState<DataModel>>>, _window_id: WindowId) -> UpdateScreen {
        app_state.data.last_time.1 = Instant::now() - app_state.data.last_time.0;
        // Trigger a redraw on every frame
        UpdateScreen::Redraw
    }

    fn stop_timer(app_state: &mut AppState<DataModel>>>, _window_id: WindowId) -> UpdateScreen {
        app_state.pop_deamon("timer_1");
        UpdateScreen::Redraw
    }
}
```

Polling functions / deamons are useful when implementing actions that should run
independently if the user interacts with the application or not.

## Styling

azul comes with default styles that mimick the operating-system native style.
However, you can overwrite parts (or everything) with your custom CSS styles:

```rust
let default_css = Css::native();
let my_css = Css::new_from_string(include_str!("my_custom.css"));
// Use the default CSS as a fallback, but overwrite only the styles in my_custom.css
let custom_css = default_css + my_css;
```

The default styles are implemented using CSS classes, with the special name
`.__azul-native-<node type>`, i.e. `__azul-native-button` for styling buttons,
`.__azul-native-scrollbar` for styling scrollbars, etc.

You can add and remove CSS styles dynamically using `my_style.push_css_rule(CssRule::new("color", "#fff"));`.
However, this will trigger a re-build of the CSS rules, relayout and re-style, and is
generally not recommended. It is recommended that you don't over-use this feature and rather switch out
CSS blocks in the `get_dom()` method, rather than changing CSS properties:

```css
.btn-active { color: blue; }
.btn-danger { color: red; }
```
```rust
if self.button[i].is_danger {
    dom.class("btn-danger");
} else {
    dom.class("btn-active");
}
```

instead of:

```rust
if self.button[i].is_danger {
    app_data.windows[window_id].css.push_rule(".btn-active { color: red; }");
} else {
    // warning: pushing CSS is stateful and won't be re-generated every frame
    // DONT DO THIS, VERY BAD PERFORMANCE
    app_data.windows[window_id].css.push_rule(".btn-active { color: blue; }");
}
```

## SVG / Canvas API

**NOTE: This README was written for the future, not implemented yet**

For drawing custom graphics, azul has a high-performance 2D vector & raster API.
The core of the custom-drawing API is based on an OpenGL texture. A `NodeType::SvgComponent`
consists of "layers", like in Photoshop, which are OpenGL textures composited on top
of each other. To make it easier to display vector graphics, you can directly initialize
a component from a SVG file (uses the `resvg` parser, `usvg` minifaction, `lyon` triangulation
and `glium` drawing libraries):

```rust
// Don't parse and SVG in the `get_dom()` function, store the parsed SVG in
// the data model, to cache it
let svg_parsed = Svg::new_from_string(include_str!("hello.svg"));

dom.add_child
    Dom::new(NodeType::DrawComponent(
        DrawComponent::Svg {
            layers: vec![
                ("layer-01", svg_parsed.into(), SvgCallbacks::None)
            ],
        }
    ))
    .with_id("my-svg")
);
```

If you want callbacks on any item **inside** the SVG, i.e. when someone clicks or hovers on / over a shape,
you can register a callback for that, using the `SvgCallbacks`.

// TODO: explain how to register custom events

In order to draw your own vector graphics, without putting the data through an SVG parser
first, you can build the layers yourself (ex. from the SVG data).

Since azul needs the image library anyway (for decoding), it is re-exported, to improve build times
and reduce duplication (so you don't have to do `extern crate image`, just do `use azul::image;`)

## Other features

### Current

- Supported CSS attributes (syntax is the same as CSS, expect when marked otherwise):
    - `background-color`
    - `background`: **Note**: `image()` takes an ID instead of a URL, see below.
    - `color`
    - `border-radius`
    - `font-size`
    - `font-family`: **Note**: same as with the `background` property, you need to register fonts first, see below.
    - `text-align`: **Note**: block-text is not supported.
    - `width`
    - `height`
    - `min-width`
    - `min-height`
    - `flex-direction`: **Note**: not implemented yet
    - `flex-wrap`: **Note**: not implemented yet
    - `justify-content`: **Note**: not implemented yet
    - `align-items`: **Note**: not implemented yet
    - `align-content`: **Note**: not implemented yet

Remarks:

1. Any measurements can be given in `px` or `em`. `px` does not respect high-DPI scaling, while `em` does.
   The default is `1em` = `16px * high_dpi_scale`
2. Images and fonts are external resources that have to be cached. Use `app.add_image("id", my_image_data)`
   or `app_state.add_image()`, then you can use the `"id"` that you gave the image in the CSS.
   If an image is not present on a displayed div (i.e. you added it to the CSS, but forgot to add the image),
   the following happens:
   - In debug mode, the app crashes with a message (to notify you of the failure)
   - In release mode, the app doesn't display the image (how could it?) and silently fails
   The same goes for fonts (azul does currently not load any default font, but that is subject to change)
3. CSS rules are (within a block), parsed from top to bottom, so:
   ```css
   #my_div {
       background: image("Cat01");
       background: linear-gradient("");
   }
   ```
   ... will give you the linear gradient, not display the image.
4. Maybe the most important thing, cascading is currently extremely buggy,
   the best result is done if you do everything via classes / ids and not mix them:
   ```css
   /* Will work */
   .general .specific .very-specific { color: black; }
   /* Won't work */
   .general #specific .very-specific { color: black; }
   ```
   The CSS parser currently only supports CSS 2.1, not CSS 3 attributes (esp. animations)
   Animations are a feature to be implemented.

### Planned

- Animations (should be implemented in CSS, not in Rust, no breaking change necessary)
- WEBM Video playback (using libvp9 / rust-media, will be exposed the same way as images, using IDs, no breaking change)
- Asynchronous callbacks (waiting on rustc to stabilize async / await)
- Looping / polling functions (important to drive futures Executors / update the app state continouusly)

## CPU & Memory usage

azul checks for all the displays in an infinite loop. Windows run by default at
60 FPS, but you can limit / unlimit this in the `WindowCreateOptions`. With these
default settings, azul uses ~ 0 - 0.5% CPU and ~ 39MB RAM. However, if you add images
and fonts, the data for these has to be kept in memory (with the uncompressed RGBA
values), so the memory usage can spike to 60MB or more once images are involved.
The redraw time (when using hardware acceleration) lies between 2 and 4 milliseconds,
i.e. 400 - 200 FPS. However, azul will only redraw the screen when absolutely necessary,
so the real FPS is usually much lower. This is usually fast enough for most desktop
applications, but not for games. However, if you use the SVG API (which skips the layout step and
uses absolute positioning), this library may be fast enough for drawing the UI in your game (~ 1 ms).

The startup time depends on how many fonts / images you add on startup, the default
time is between 100 and 200 ms for an app with no images and a single font.

## License

This library is MIT-licensed. It was developed by [Maps4Print](http://maps4print.com/),
for quickly prototyping and producing desktop GUI cross-platform applications,
such as vector or photo editors.

For licensing questions, please contact opensource@maps4print.com