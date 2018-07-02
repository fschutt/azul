# azul

# WARNING: The features advertised don't work yet.
# See the /examples folder for an example of what's currently possible.

[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Build Status Linux / macOS](https://travis-ci.org/maps4print/azul.svg?branch=master)](https://travis-ci.org/maps4print/azul)
[![Build status Windows](https://ci.appveyor.com/api/projects/status/p487hewqh6bxeucv?svg=true)](https://ci.appveyor.com/project/fschutt/azul)
[![coveralls](https://coveralls.io/repos/github/maps4print/azul/badge.svg?branch=master)](https://coveralls.io/github/maps4print/azul?branch=master)
[![codecov](https://codecov.io/gh/maps4print/azul/branch/master/graph/badge.svg)](https://codecov.io/gh/maps4print/azul)
[![Rust Compiler Version](https://img.shields.io/badge/rustc-1.26%20stable-blue.svg)]()

azul is a cross-platform, stylable GUI framework using Mozillas `webrender`
engine for rendering and a CSS / DOM model for layout and rendering

[Crates.io](https://crates.io/crates/azul) | 
[Library documentation](https://docs.rs/azul) | 
[User guide](http://azul.rs/)

## Installation notes

On Linux, you currently need to install `cmake` before you can use azul. 
CMake is used during the build process to compile servo-freetype. 

```
sudo apt install cmake
```

## Design

azul is a library designed from the experience gathered during working with
other GUI toolkits. azul is very different from (QT / GTK / FLTK / etc.) in the
following regards:

- GUIs are seen as a "view" into your applications data, they are not "objects" 
  like in any other toolkit. There is no `button.setActive(true)` for example, 
  as that would introduce stateful design.
- Widgets types are simply enums that "serialize" themselves into a DOM tree.
- The DOM is immutable and gets re-generated every frame. This makes testing
  and debugging very easy, since if you give the `get_dom()` function a
  specific data model, you always get the same DOM back (`get_dom()` is a pure 
  function). This comes at a slight performance cost, however in practice the 
  cost isn't too high and it makes the seperation of application data and GUI 
  data very clean.
- The layout model closely follows the CSS flexbox model. The default for CSS is
  `display:flex` instead of `display:static` (keep that in mind). Some semantics
  of CSS are not the same, especially the `image` and `vertical-align` properties.
  However, most attributes work in azul, same as they do in CSS, i.e. `color`,
  `linear-gradient`, etc.
- azul trades a slight bit of performance for better usability. azul is not meant
  for game UIs, it is currently too slow for that (currently using 2 - 4 ms per 
  frame)
- azul links everything statically, including freetype and OSMesa (in case the target
  system has no hardware accelerated drawing available)

## Data model / Reactive programming

To understand how to efficiently program with azul, you have to understand its
programming / memory model. One image says more than 1000 words:

![Azul callback model](https://raw.githubusercontent.com/maps4print/azul/master/doc/azul_callback_model.png)

This creates a very simple programming flow:

```rust
use azul::prelude::*;
use azul::widgets::*;

// Your data model that stores everything except the visual 
// representation of the app
#[derive(Default)]
struct DataModel {
    // Store anything relevant to the application here 
    // i.e. settings, a counter, user login data, application data
    // the current application zoom, whatever. 
    //
    // This decouples visual components from another - instead of
    // updating the visual representation of another component 
    // on an event, they only update the data they operate on.
    //
    // For example:
    user_name: Option<(Username, Password)>
}

// Data model -> DOM
impl Layout for DataModel {
    fn layout(&self, _info: WindowInfo) -> Dom<DataModel> {
        // DataModel is read-only here, "serialize" from the data model into a UI
        // 
        // Conditional logic / updating the contents of an existing window
        // is very easy - instead of something like `screen.remove(LoginButton);`
        // `screen.add(HelloLabel)`, we can simply write:
        match self.user_name {
            None => Button::with_text("Please log in").dom()
                        .with_event(On::MouseUp, Callback(login_callback)),
            Some((user, _)) => Label::with_text(format!("Hello {}", user)).dom() 
        }
    }
}

// Callback updates data model, when the button is clicked
fn login_callback(app_state: &mut AppState<DataModel>) -> UpdateScreen {
    // Let's just log the user in once he clicks the button
    app_state.data.user_name = Some(("Jon Doe", "12345"));
    UpdateScreen::Redraw
}

fn main() {
    // Initialize the initial state of the app
    let mut app = App::new(DataModel::default());
    // Create as many initial windows as you want
    app.create_window(WindowCreateOptions::default(), Css::native()).unwrap();
    // Run it!
    app.run().unwrap();
}
```

This makes it easy to compose the UI from a set of functions, where each 
function creates a sub-DOM that can be composed into a larger UI:

```rust
impl Layout for DataModel {
    fn layout(&self, _window_id: WindowId) -> Dom<DataModel> {
        if !self.is_email_sent {
            Dom::new(NodeType::Div)
                .with_child(email_recipients_list(&self.names));
                .with_child(email_send_button());
        } else {
            Dom::new(NodeType::Div)
                .with_child(no_email_label());
        }
    }
}

fn email_recipients_list(names: &[String]) -> Dom<DataModel> {
    let mut names_list = Dom::new(NodeType::Div);
    for name in names {
        names_list.add_child(Label::new(name).dom());
    }
    names_list
}

fn email_send_button() -> Dom<Self> {
    Button::labeled("Send email").dom()
        .with_id("email-send-button")
        .with_event(On::MouseUp, Callback(send_email))
}

fn no_email_label() -> Dom<Self> {
    Button::labeled("No email to send!").dom()
        .with_id("no-email-label")
}

fn send_email(app_state: &mut AppState<DataModel>, _: WindowEvent) -> UpdateScreen {
    app_state.data.is_email_sent = false;
    UpdateScreen::Redraw
}
```

And this is why azul doesn't really have a large API like other frameworks -
that's really all there is to it! The widgets themselves might require you to
pass a cache or some state across into the `.dom()` function, but the core 
model doesn't change. Meaning, if you remove / add visual components, it 
doesn't break the whole application as long as the data model stays the same.
A visual component has no knowledge of any other components, it only acts on 
the data model.

The benefit of this is that it's very simple to refactor and to test:

```rust
#[test]
fn test_it_should_send_the_email() {
    let mut initial_state = AppState::new(DataModel { is_email_sent: false });
    send_email(&mut initial_state, WindowId::new(0));
    assert_eq!(initial_state.is_email_sent, true);
}
```

As well as to write DOM  / visual regression tests:

```rust
#[test]
fn test_layout_email_dom() {
    let dom = DataModel { is_email_sent: false }.layout();
    let arena = dom.arena.borrow();

    let expected = NodeType::Label(String::from("Send email"));
    let got = arena[arena[dom.root].first_child().unwrap()].data.data;

    assert_eq!(expected, got);
}
```

The inner workings of the DOM are only available in testing functions, not 
regular code. 

You might have noticed that a `WindowEvent` gets passed to the callback. 
This struct contains callback information that is necessary to determine what 
item (of a bigger list, for example) was interacted with. Ex. if you have 
100 list items, you don't want to write 100 callbacks for each one, but rather
have one callback that acts on which ID was selected. The `WindowEvent` 
gives you the necessary information to react to these events.

## Updating window properties

You may also have noticed that the callback takes in a `AppState<DataModel>`,
not the `DataModel` directly. This is because you can change the window 
settings, for example the title of the window:

```rust
fn callback(app_state: &mut AppState<DataModel>, _: WindowEvent) -> UpdateScreen {
    app_state.windows[window_id].window.title = "Hello";
    app_state.windows[window_id].window.menu += "&Application > &Quit\tAlt+F4";
}
```

Note how there isn't any `.get_title()` or `.set_title()`. Simply setting the 
title is enough to invoke the (stateful) Win32 / X11 / Wayland / Cocoa functions 
for setting the window title. You can query the active title / mouse or keyboard 
state in the same way.

## Async I/O

When you have to perform a larger task, such as waiting for network content or
waiting for a large file to be loaded, you don't want to block the user 
interface, which would give a bad experience. 

Instead, azul provides two mechanisms: a `Task` and a `Deamon`. Both are 
essentially function callbacks, but the `Task` gets run on a seperate thread 
(one thread per task) while a `Deamon` gets run on the same thread as the main
UI.

azul takes care of querying if the `Task` or `Deamon` has finished. Both have
access to the applications data model and can modify it (without race conditions):

```rust
use std::{thread, time::Duration};

struct DataModel {
    website: Option<String>,
}

impl Layout for DataModel {
    fn layout(&self, _: WindowInfo) -> Dom<DataModel> {
        match self.website {
            None => 
                Dom::new()
                    .with_child(Button::labeled("Click to download").dom())
                    .with_event(On::MouseUp, Callback(start_download))
            Some(data) => 
                Dom::new()
                    .with_child(Label::new(data.clone()).dom()),
        }
    }
}

fn start_download(app_state: &mut AppState<DataModel>, _: WindowEvent) -> UpdateScreen {
    app_state.add_task(download_website);
    UpdateScreen::DontRedraw
}

fn download_website(app_state: Arc<Mutex<AppState<DataModel>>>, _drop: Arc<()>) {
    // simulate slow, blocking IO
    thread::sleep(Duration::from_secs(5));
    app_state.modify(|data| data.website = Some("<h1>Hello</h1>".into()));
}
```

The `app_state.modify` is a only conveniece function that locks and unlocks your
data model. The `_drop` variable is necessary so that azul can see when the thread
has finished and join it afterwards. 

A `Task` starts one full, OS-level thread. Usually doing this is a bad idea for 
performance, since you may at one point have too many threads running. However, in
desktop applications, you usually don't run  1000 tasks at once, maybe 4 - 5 maximum.
For this, OS-level threads are usually sufficient and performant enough.

## Styling

Azul has default visual styles that mimick the sperating-systems native style.
However, you can overwrite parts (or everything) with your custom CSS styles:

```rust
let default_css = Css::native();
let my_css = Css::new_from_string(include_str!("my_custom.css")).unwrap();
let custom_css = default_css + my_css;
```

The default styles are implemented using CSS classes, with the special name
`.__azul-native-<node type>`, i.e. `__azul-native-button` for styling buttons,
`.__azul-native-scrollbar` for styling scrollbars, etc.

## Dynamic CSS properties

You can override CSS properties from Rust during runtime, but after every frame
the modifications are cleared again. You do not have to "unset" a CSS style once the 
state of your application changes. Example:

```rust
struct Discord { light_theme: bool }

impl Layout for Discord {
    fn layout(&self, _: WindowInfo) -> Dom<Discord> {
        Dom::new(NodeType::Div)
            .with_class("background")
            .with_event(On::MouseOver, Callback(mouse_over_window))
    }
}

fn mouse_over_window(_: &mut AppState<Discord>, window: WindowEvent) -> UpdateScreen {
    window.css.set("theme_background_color", ("color", "#fff")).unwrap();
    UpdateScreen::Redraw
}

fn main() {
    let css = Css::new_from_string("
        .hovered {
            color: [[ theme_background_color | #333 ]];
        }
    ").unwrap();
    let mut app = App::new(Discord { light_theme: false })
    app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run().unwrap();
}
```

The `[[ variable | default ]]` denotes a "dynamic CSS property". If the 
`theme_background_color` variable isn't set for this frame, it will use the 
default. The reason why the CSS state is unset on every frame is to prevent 
forgetting to unset it. Especially with highly conditional styling, this 
can easily lead to bugs (i.e. only set this button to green if a certain setting
is set to true, but not if today is Wednesday).

The second reason is to make CSS properties testable, i.e. in the same way that
you can test state properties, you can test CSS properties. Azuls philosophy is
that the state of the previous frame never affects the current frame. Only the
data model can affect the visual content, there is no `.setBackground("blue")` 
because at some point, you'd have to write a function to un-set the background 
again - this stateful design can quickly lead to visual bugs.

## SVG / OpenGL API

For drawing custom graphics, azul has a high-performance 2D vector API. It also 
allows you to load and draw SVG files (with the exceptions of gradients: gradients
in SVG files are not yet supported). But azul itself does not know about SVG shapes
at all - so how the SVG widget implemented?

The solution is to draw the SVG to an OpenGL texture and hand that to azul. This
way, the SVG drawing component could even be implemented in an external crate, if
you really wanted to. This mechanism also allows for completely custom drawing 
(let's say: a game, a 3D viewer, etc.) to be drawn.

The SVG component currently uses  the `resvg` parser, `usvg` minifaction and the
`lyon` triangulation libraries). Of course you can also add custom shapes 
(bezier curves, circles, lines, whatever) programatically, without going through
the SVG parser:

```rust
const TEST_SVG: &str = include_str!("tiger.svg");

impl Layout for Model {
    fn layout() {
        if let Some((svg_cache, svg_layers)) = self.svg {
            Svg::with_layers(svg_layers).dom(&info.window, &svg_cache)
        } else {
             Button::labeled("Load SVG file").dom()
                .with_callback(load_svg)
        }
    }
}

fn load_svg(app_state: &mut AppState<MyAppData>, _: WindowEvent) -> UpdateScreen {
    let mut svg_cache = SvgCache::empty();
    let svg_layers = svg_cache.add_svg(TEST_SVG).unwrap();
    app_state.data.modify(|data| data.svg = Some((svg_cache, svg_layers)));
    UpdateScreen::Redraw
}
```

This is one of the few exceptions where azul allows persistent data across frames
since it wouldn't be performant enough otherwise. Ideally you'd have to load, triangulate 
and draw the SVG file on every frame, but this isn't performant. You might have
noticed that the `.dom()` function takes in an extra parameter: The `svg_cache`
and the `info.window`. This way, the `svg_cache` handles everything necessary to
cache vertex buffers / the triangulated layers and shapes, only the drawing itself
is done on every frame.

Additionally, you can also register callbacks on any item **inside** the SVG using the 
`SvgCallbacks`, i.e. when someone clicks on or hovers over a certain shape. In order 
to draw your own vector data (for example in order to make a vector graphics editor),
you can build the "SVG layers" yourself (ex. from the SVG data). Each layer is 
batch-rendered, so you can draw many lines or polygons in one draw call, as long as
they share the same `SvgStyle`.

## Supported CSS attributes

### Implemented

This is a list of CSS attributes that are currently implemented. They work in 
the same way as on a regular web page, except if noted otherwise:

- `border-radius`
- `background-color`
- `color`
- `border`
- `background`                              [see #1]
- `font-size`
- `font-family`                             [see #1]
- `box-shadow`
- `line-height`
- `width`, `min-width`, `max-width`
- `height`, `min-height`, `max-height`
- `align-items`                             [see #2]
- `overflow`, `overflow-x`, `overflow-y`
- `text-align`                              [see #3]

Notes:

1. `image()` takes an ID instead of a URL. Images and fonts are external resources 
    that have to be cached. Use `app.add_image("id", my_image_data)` or 
    `app_state.add_image()`, then you can use the `"id"` in the CSS to select 
    your image.
    If an image is not present on a displayed div (i.e. you added it to the CSS, 
    but forgot to add the image), the following happens:
    - In debug mode, the app crashes with a descriptive error message
    - In release mode, the app doesn't display the image and logs the error
2.  Currently `align-items` is only implemented to center text vertically
3.  Justified text is not (yet) supported

### Planned

These properties are planned for the next (currently 0.1) release:

- `flex-wrap`
- `flex-direction`
- `justify-content`
- `align-content`

### Remarks

1. Any measurements can be given in `px`, `pt` or `em`. `pt` does not 
   respect high-DPI scaling, while `em` and `px` do. The default is `1em` = 
   `16px * high_dpi_scale`
2. CSS rules are (within a block), parsed from top to bottom, ex:
   ```css
   #my_div {
       background: image("Cat01");
       background: linear-gradient("");
   }
   ```
   This will draw a linear gradient, not the image, since the `linear-gradient` rule
   overwrote the `image` rule.
3. Maybe the most important thing, cascading is currently extremely buggy,
   the best result is done if you do everything via classes / ids and not mix them:
   ```css
   /* Will work */
   .general .specific .very-specific { color: black; }
   /* Won't work */
   .general #specific .very-specific { color: black; }
   ```

The CSS parser currently only supports CSS 2.1, not CSS 3 attributes. Animations 
are not done in CSS, but rather by using dynamic CSS properties (see above)

### Planned

- WEBM Video playback (using libvp9 / rust-media, will be exposed the same way as 
  images, using IDs, no breaking change)

## CPU / memory usage / startup time

While efficiency is definitely one goal, ease of use is the first and foremost
goal. In order to respond to events, azul runs in an infinite loop and checks all
windows for incoming events, which consumes around 0-0.5% CPU when idling.

A default window, with no fonts or images added takes up roughly 39MB of RAM and
5MB in binary size. This usage can go up once you load more images and fonts, since
azul has to load and keep the images in RAM. 

The frame time (i.e. the time necessary to draw a single frame, including layout) 
lies between 2 - 5 milliseconds, which equals roughly 200 - 500 frames per second. 
However, azul limits this frame time and only redraws the window when absolutely 
necessary, in order to not waste the users battery life. 

The startup time depends on how many fonts / images you add on startup, the
default time is between 100 and 200 ms for an app with no images and a single font.

## Thanks

Several projects have helped azul severely and should be noted here:

- Chris Tollidays [limn](https://github.com/christolliday/limn) framework has helped
  severely with discovering undocumented parts of webrender as well as working with
  constraints (the `constraints.rs` file was copied from limn with the [permission of
  the author](https://github.com/christolliday/limn/issues/22#issuecomment-362545167))
- Nicolas Silva for his work on [lyon](https://github.com/nical/lyon) - without this, 
  the SVG renderer wouldn't have been possible
- All webrender contributors who have been patient enough to answer my questions on IRC

## License

This library is MIT-licensed. It was developed by [Maps4Print](http://maps4print.com/),
for quickly prototyping and producing desktop GUI cross-platform applications,
such as vector or photo editors.

For licensing questions, please contact opensource@maps4print.com