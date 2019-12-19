# Azul - Desktop GUI framework

## WARNING: The features advertised in this README may not work yet.

<!-- [START badges] -->
[![Build Status Linux / macOS](https://travis-ci.org/maps4print/azul.svg?branch=master)](https://travis-ci.org/maps4print/azul)
[![Build status Windows](https://ci.appveyor.com/api/projects/status/p487hewqh6bxeucv?svg=true)](https://ci.appveyor.com/project/fschutt/azul)
[![Coverage Status](https://coveralls.io/repos/github/maps4print/azul/badge.svg?branch=master)](https://coveralls.io/github/maps4print/azul?branch=master)
[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE) [![Rust Compiler Version](https://img.shields.io/badge/rustc-1.33%20stable-blue.svg)]()
<!-- [END badges] -->

> Azul is a free, functional, immediate mode GUI framework that is built on the Mozilla WebRender rendering engine for rapid development
of desktop applications that are written in Rust and use a CSS / DOM model for layout and styling.

###### [Website](https://azul.rs/) | [Tutorial / user guide](https://github.com/maps4print/azul/wiki) | [Video demo](https://www.youtube.com/watch?v=kWL0ehf4wwI) | [Discord Chat](https://discord.gg/nxUmsCG)

## About

Azul is a library for creating graphical user interfaces or GUIs in Rust. It mixes
paradigms from functional, immediate mode GUI programming commonly found in games
and game engines with an API suitable for developing desktop applications.
Instead of focusing on an object-oriented approach to GUI programming ("a button
is an object"), it focuses on combining objects by composition ("a button is a function")
and achieves complex layouts by composing widgets into a larger DOM tree.

Azul separates the concerns of business logic / callbacks, data model and UI
rendering / styling by not letting the UI / rendering logic have mutable access
to the application data. Widgets of your user interface are seen as a "view" into
your applications data, they are not "objects that manage their own state", like
in so many other toolkits. Widgets are simply functions that render a certain state,
more complex widgets combine buttons by calling a function multiple times.

The generated DOM itself is immutable and gets re-generated every frame. This makes testing
and debugging very easy, since the UI is a pure function, mapping from a specific application
state into a visual interface. For layouting, Azul features a custom CSS-like layout engine,
which closely follows the CSS flexbox model.

## Hello World

Here is what a Hello World application in Azul looks like:

![Hello World Application](https://i.imgur.com/KkqB2E5.png)

This application is created by the following code:

```rust
extern crate azul;

use azul::{
    prelude::*,
    widgets::{button::Button, label::Label},
};

struct CounterApp {
    counter: usize,
}

impl Layout for CounterApp {
    fn layout(&self, _: LayoutInfo<Self>) -> Dom<Self> {
        let label = Label::new(format!("{}", self.counter)).dom();
        let button = Button::with_label("Update counter")
            .dom()
            .with_callback(On::MouseUp, update_counter);

        Dom::div().with_child(label).with_child(button)
    }
}

fn update_counter(event: CallbackInfo<CounterApp>) -> UpdateScreen {
    event.state.data.counter += 1;
    Redraw
}

fn main() {
    let mut app = App::new(CounterApp { counter: 0 }, AppConfig::default()).unwrap();
    let window = app
        .create_window(WindowCreateOptions::default(), css::native())
        .unwrap();
    app.run(window).unwrap();
}
```

[Read more about the Hello-World application ...](https://github.com/maps4print/azul/wiki/A-simple-counter)

## Programming model

In order to comply with Rust's mutability rules, the application lifecycle in Azul
consists of three states that are called over and over again. The framework determines
exactly when a repaint is necessary, you don't need to worry about manually repainting
your UI:

![Azul callback model](https://i.imgur.com/cTTULrP.png)

Azul works through composition instead of inheritance - widgets are composed of other
widgets, instead of inheriting from them (since Rust does not support inheritance).
The main `layout()` function of a production-ready application could look something
like this:

```rust
impl Layout for DataModel {
    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<DataModel> {
        match self.state {
            LoginScreen => {
                Dom::new(NodeType::Div).with_id("login_screen")
                    .with_child(render_hello_mgs())
                    .with_child(render_login_with_button())
                    .with_child(render_password())
                    .with_child(render_username_field())
            },
            EmailList(emails) => {
                Dom::new(NodeType::Div).with_id("email_list_container")
                    .with_child(render_task_bar())
                    .with_child(emails.iter().map(render_email).collect())
                    .with_child(render_status_bar())
            }
        }
    }
}
```

One defining feature is that Azul automatically determines when a UI repaint is
necessary and therefore you don't need to worry about manually redrawing your UI.

[Read more about the programming model ...](https://github.com/maps4print/azul/wiki/Getting-Started)

## Features

### Easy two-way data binding

When programming reusable and common UI elements, such as lists, tables or sliders
you don't want the user having to write code to update the UI state of these widgets.
Previously, this could only be solved by inheritance, but due to Azul's unique
architecture, it is possible to create widgets that update themselves purely by
composition, for example:

```rust
struct DataModel {
    text_input: TextInputState,
}

impl Layout for DataModel {
    fn layout(&self, info: LayoutInfo<Self>) -> Dom<Self> {
        // Create a new text input field
        TextInput::new()
        // ... bind it to self.text_input - will automatically update
        .bind(info.window, &self.text_input, &self)
        // ... and render it in the UI
        .dom(&self.text_input)
        .with_callback(On::KeyUp, Callback(print_text_field))
    }
}

fn print_text_field(app_state: &mut AppState<DataModel>, _event: &mut CallbackInfo<DataModel>) -> UpdateScreen {
    println!("You've typed: {}", app_state.data.lock().unwrap().text_input.text);
    DontRedraw
}
```

[Read more about two-way data binding ...](https://github.com/maps4print/azul/wiki/Two-way-data-binding)

### CSS styling & layout engine

Azul features a CSS-like layout and styling engine that is modeled after the
flexbox model - i.e. by default, every element will try to stretch to the dimensions
of its parent. The layout itself is handled by a simple and fast flexbox layout solver.

[Read more about CSS styling ...](https://github.com/maps4print/azul/wiki/Styling-your-application-with-CSS)

### Asynchronous UI programming

Azul features multiple ways of preventing your UI from being blocked, such as
"Tasks" (threads that are managed by the Azul runtime) and "Daemons"
(callback functions that can be optionally used as timers or timeouts).

[Read more about async IO ...](https://github.com/maps4print/azul/wiki/Timers,-daemons,-tasks-and-async-IO)

### SVG / GPU-accelerated 2D Vector drawing

For drawing non-rectangular shapes, such as triangles, circles, polygons or SVG files,
Azul provides a GPU-accelerated 2D renderer, featuring lines drawing (incl. bezier curves),
rects, circles, arbitrary polygons, text (incl. translation / rotation and text-on-curve
positioning), hit-testing texts, caching and an (optional) SVG parsing module.

![Azul SVG Tiger drawing](https://i.imgur.com/JQvtmxA.png)

[Read more about SVG drawing ...](https://github.com/maps4print/azul/wiki/SVG-drawing)

### OpenGL API

While Azul can't help you (yet) with 3D content, it does provide easy ways to hook
into the OpenGL context of the running application - you can draw everything you
want to an OpenGL texture, which will then be composited into the frame using
WebRender.

[Read more about OpenGL drawing ...](https://github.com/maps4print/azul/wiki/OpenGL-drawing)

### UI Testing

Due to the separation of the UI, the data model and the callbacks, Azul applications
are very easy to test:

```rust
#[test]
fn test_it_should_increase_the_counter() {
    let mut initial_state = AppState::new(DataModel { counter: 0 });
    let expected_state = AppState::new(DataModel { counter: 1 });
    update_counter(&mut initial_state, &mut CallbackInfo::mock());
    assert_eq!(initial_state, expected_state);
}
```

[Read more about testing ...](https://github.com/maps4print/azul/wiki/Unit-testing)

## Performance

A default window, with no fonts or images added takes up roughly 23MB of RAM and
5MB in binary size. This usage can go up once you load more images and fonts, since
Azul has to load and keep the images in RAM.

The frame time (i.e. the time necessary to draw a single frame, including layout)
lies between 2 - 5 milliseconds, which equals roughly 200 - 500 frames per second.
However, Azul limits this frame time and **only redraws the window when absolutely
necessary**, in order to not waste the users battery life.

The startup time depends on how many fonts / images you add on startup, the
default time is between 100 and 200 ms for an app with no images and a single font.

While Azul can run in software rendering mode (automatically switching to the
built-in OSMesa), it isn't intended to run on microcontrollers or devices with
extremely low memory requirements.

## Thanks

Several projects have helped severely during the development and should be credited:

- Chris Tollidays [limn](https://github.com/christolliday/limn) framework has helped
  a lot with discovering undocumented parts of WebRender.
- Nicolas Silva for his work on [lyon](https://github.com/nical/lyon) - without this,
  the SVG renderer wouldn't have been possible

## License

This library is MIT-licensed. It was developed by [Maps4Print](http://maps4print.com/),
for quickly prototyping and producing desktop GUI cross-platform applications,
such as vector or photo editors.

For licensing questions, please contact opensource@maps4print.com
