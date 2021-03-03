In this tutorial we are going to build a simple application where the user can
click on a button to increment a counter. It shows how to create a data model,
create a UI for said data model and react to user input.

## The data model

As in the previous tutorial, we're going to start designing what the data model
should look like. In our case it's going to be really simple:

```rust
use azul::{prelude::*, widgets::{label::Label, button::Button}};

struct CounterApplication {
    counter: usize,
}
```

Once your app grows beyond the hello-world case, you will possibly also store
UI-related information in here, i.e. how many pixels wide a certain window pane
is open and so on.

## Layout

The layout part is split in two parts: The CSS and the DOM tree. For rapid
development, azul has a XML-based mode which lets you iterate quickly on the
UI layout of new features and compile XML to Rust, so you don't have to suffer
the problem of long compile times which can hinder rapid UI development.
However, right now we will focus on doing it in pure Rust instead of mixed
Rust / XML. 

```rust
impl Layout for CounterApplication {
    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<Self> {
        let label = Label::new(format!("{}", self.counter)).dom();
        let button = Button::with_label("Update counter").dom()
            .with_callback(On::MouseUp, update_counter);

        Dom::div()
            .with_child(label)
            .with_child(button)
    }
}
```

Notice the `.dom()` function on the `Button` and the `Label`. This 
function name is just a convention - most standard widgets have a 
`.dom()` function, which converts the `Button` into a `Dom`. There
is no special trait for this because some functions need to take
additional parameters into account, for example the `Svg` widget
needs access to an `SvgCache` and access to the current window, since
it needs to manage and draw OpenGL and cache SVG polygon shapes.
However, not every widget needs the same type of parameters, so the 
`.dom()` function is simply a conventionally named function that
transforms the given widget into a DOM.

Any DOM node can be styled via CSS and the standard widgets simply
have special CSS class names attached to them - if you wanted to,
you could overwrite the layout of standard widgets, but for now,
we are going to leave everything at the default.

The goal here is to compose widgets / layouts by appending DOM
objects to each other. Abstractions such as the `Button` and 
`Label` can then be built on top of that, so instead of writing:

```rust
Dom::new(NodeType::Div)
.with_class("__azul-native-label")
.with_child(Dom::label("Hello".into()))
```

... we can write:

```rust
Label::new("Hello").dom()
```

... which is shorter and more descriptive. Through this mechanism you 
can build your UI by composition instead of inheritance, composing widgets
into your own widget types, so that you can build really high-level abstractions
and let the rest get handled by the widget itself.

This also allows you to dynamically show / hide widgets, based on the current
application state:

```rust
if self.should_show_component {
     Label::new("Component is shown!").dom()
} else {
     Label::new("Component is hidden!").dom()
}
```

Azul automatically hashes and diffs the `Dom` - while there is
a small performance hit for re-creating the `Dom` every frame, Rust is
fast enough that you'll likely never notice it.

Composing these widgets into a `Dom` can be done in several ways:

- `add_child(child_dom)` - appends the new DOM as a child to the current DOM
- `x_list.iter().map(|x| Dom).collect()` - very good for lists / maps, appending each DOM node as a sibling to the previous one

For example:

```rust
(0..5).map(|number| Label::new(format!("{}", number + 1)).dom()).collect()
```
... will build a list with the numbers `1, 2, 3, 4, 5` in seperate labels.
Later on you can then layout this list horizontally or vertically or
however you like in CSS. Or you could do:

```rust
(0..5).map(|number| {
    Dom::div()
    .with_class(if number % 2 == 0 { "even" } else { "odd" })
}).collect()
```

... this would allow you to style the component from CSS based on even / 
odd-ness, for example to get alternating colors (note: Azul supports `:even`, 
`:odd` and `:nth-child()` pseudo-selectors, you don't need to do it for 
even- / odd-ness, this was just an example of how flexible the immediate-mode 
styling is).

A thing to note is that the UI is thread-safe. While the `layout()`
function is active, no other thread has access to the data model. It is good 
practice to **not** cheat the borrow checker by using `Rc` or `RefCell` to update
your data model inside the `.layout()` function.

This should be enough information about the DOM, later on we'll get into
performance optimization, things to consider and best practices when working
with the `Dom`. For now, let's see how we can make our UI actually do something.

## Handling callbacks

To recap, here's what our app looks like right now:

```rust
use azul::{prelude::*, widgets::{label::Label, button::Button}};

struct CounterApplication {
    counter: usize,
}

impl Layout for CounterApplication {
    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<Self> {
        let label = Label::new(format!("{}", self.counter)).dom();
        let button = Button::with_label("Update counter").dom()
            .with_callback(On::MouseUp, update_counter);

        Dom::div()
            .with_child(label)
            .with_child(button)
    }
}
```

You might have already noticed the `.with_callback()` method with the mysterious 
`update_counter` argument. `update_counter` is the name of a function that we 
haven't written yet. Callbacks are internally function pointers, but they are not
the same as callbacks in other languages or frameworks, which is a defining difference
of azul. 

In many other frameworks, you do something like this (pseudocode):

```python
def main():
    my_gui = Gui()
    button = Button()
    button.set_onclick(print_hello)
    my_gui.add_button(button)
    my_gui.run()

def print_hello():
    print("Hello World")
```

... while this makes for very short Hello-World examples this model
has one big problem: Where is the application data being stored? 
How can the callback change the data of other widgets or communicate
between widgets (i.e. a text input updating a text label)? Many frameworks
leave this up as an "excercise to the reader", which leads to very crude
solutions involving static mutable data, global variables and 
complex inheritance hierarchies or meta-compilers that modify your source
code to make passing messages between widgets somewhat bearable.

Azul does a very simple thing: Since it already owns the entire data model,
it simply passes a mutable reference to the data model to any callback.
This means that any callback can change any component in the app model.
While this may sound scary and unmaintainable at first, in practice it works
very well due to Rusts mutability guarantees. Communication between widgets
works based on the shared memory of the data model, for example:

If we have a visual widget `TextInput` and a widget `Label`, they don't know
about each other. The `TextInput` only knows that it should update a `String`
in the data model, and the `Label` only knows that it shoul re-render a certain 
`String`. But there is no `label.set_text(text_input->get_current_text())`, 
because that would require the label to know about the text input field, 
thereby coupling them together.

A callback function has to have a certain signature (the arguments it takes):
It takes a simple `CallbackInfo<T>` (which internally contains a mutable reference
to the `AppState<T>`, so that you can access your data).

```rust
fn my_callback(event: CallbackInfo<T>) -> UpdateScreen { /**/ }
```

The first thing you've probably noticed is that we don't have a `&mut CounterApplication`,
but rather a `&mut AppState<CounterApplication>`. Why is that? Early versions of azul had
exactly that, however, the problem with this came with drawing custom shapes and text. 
For that to work, you usually need access to resources that aren't part of your data model,
such as fonts or images. The `AppState` stores these resources and the `&mut` allows you
to dynamically load / unload fonts and images. 

Another reason is multithreading - earlier I mentioned that the data model is thread-safe.
The `AppState` stores an `Arc<Mutex<CounterApplication>>` - callbacks can start threads and
hand out a clone of that `Arc` to a different thread, meaning while a callback is running
and the data model isn't locked, it is possible that a different thread is currently modifying
the data model - but more on threads later. Some resources can be shared across threads
while others can't (specifically OpenGL drawing is not thread-safe). There are some things 
that you can only do on the main thread, and other things that you can offload to other threads.
This distinction led to the current design of handing an `&mut AppState<T>` to the callbacks,
not only a `&mut T`.

The callback should return an `UpdateScreen` "enum" - `UpdateScreen` is currently a 
typedef for `Option<()>`, not an enum. This is because currently Rust doesn't allow custom
implementations for `?` operators (with the exception of Option and Result). In practice however, 
programming callbacks with out the `?` operator is very painful. Usually, if a callback
fails, you don't want the application to panic, you'd just want it to "do nothing".
Because `UpdateScreen` is just a type definition for `Option<()>`, you can use the `?` operator
without any problems. In the future, this might be reverted back to an enum if Rust implements custom try operators
(at which point the code will switch back to `UpdateScreen::Redraw` and `UpdateScreen::DontRedraw` again). 
The constants `Redraw` and `DontRedraw` (imported from the `use prelude::*;`) simply represent
`None` and `Some(())` respectively.

`UpdateScreen` serves as a performance optimization - if the returned value of all 
callbacks is set to `DontRedraw`, then azul doesn't need to redraw the screen at all. 
Azul will automatically redraw the entire screen on events such as window resizing. 
Redrawing the screen involves calling the `.layout()` function again, so a general 
rule of thumb is that your callback should only return `Redraw` if it actually changes the 
visual contents of the screen. Azul doesn't redraw the screen more than necessary, 
so you shouldn't need to worry about potential performance concerns - re-layouting and 
redrawing the screen only takes only 1 - 4 ms (linear to your apps UI complexity).

Right now, a callback needs to be wrapped in a `Callback` tuple-struct, because Rust 
has a few problems with copying and cloning generic function pointers. Additionally,
it enhances readability of the code, so it's likely to be kept in the framework anyways.

But let's look at our callback for the counter application:

```rust
fn update_counter(event: CallbackInfo<CounterApplication>) -> UpdateScreen {
    event.state.data.counter += 1;
    Redraw
}
```

A last thing to note: How would you know that the function is only called on a 
`On::MouseUp` event? Well, you can't know what event called the function - 
the callback itself has no knowledge  about what event called it, it just knows 
that it was called. This is a deliberate design choice - you should not design 
your callbacks to react only to certain events, because this makes them harder to 
test. This means that you can swap out the `On::Click` for an `On::Hover` in the layout 
function to make your counter go up when you move the mouse over it for example.

Before azul invokes any callbacks, it updates the current window state, such as 
what keys are currently pressed (in this frame), where the mouse is, how large 
the window is, etc. When working on earlier versions of azul, I noticed that what 
a lot of people need to do is to track the current state of the window and / or 
compare it with the previous frame. Doing anything else resulted in the apps data 
model get polluted with unnecessary `current_mouse_position` or similar information - 
things that weren't necessary for the application data, but that I needed to have 
access to in the callbacks. So they were moved into the framework, so that Azul
tracks these things for you.

Callbacks are currently not sorted in any particular order. There is also no 
`event.preventDefault()` to solve the problem of inner-to-outer or outer-to-inner 
callbacks - the reason for this is that this problem doesn't exist because azul passes
the entire application state to your callback. If you want to stop certain events
(which is a rare ocurrence usually), simply store a flag (or enum) in your application 
data model. Since every callback shares that model, you can run functions only if 
the application is currently in a certain state.

The rest of the application should be pretty self-explanatory: We intialize the counter to 0,
open a window and start the application:

```rust
fn main() {
    let mut app = App::new(CounterApplication { counter: 0 }, AppConfig::default()).unwrap();
    let window = app.create_window(WindowCreateOptions::default(), css::native()).unwrap();
    app.run(window).unwrap();
}
```
This should result in a window with a label and a button that look similar to the picture
below (exact style can differ depending on the operating system). If you now click the button,
the number should increase by 1 on each click.

![Azul Counter Application](https://i.imgur.com/KkqB2E5.png)

<!--
## Unit testing our app

This is one of the places that azul shines at - because the callback is a simple
function, you can test in which places the callback modifies your data model. This
partly solves the problem of scalability: How do you, in a large application verify
what callbacks work correctly in any order? How do you know what data they modify,
what state they touch?

Azul allows any callback to modify the entire application state. So how is this maintainable?
In azul, maintainability is not achieved by reducing the scope of access, 
but by using lots of small tests. Each unit test should test that a certain callback 
only changes a certain field and doesn't have access to other fields:

```rust
#[test]
fn test_it_should_increment_the_button() {
    let mut app_state = AppState::new(CounterApplication { counter: 0 });
    my_button_click_handler(CallbackInfo::new(&mut app_state));
    let expected_model = CounterApplication { counter: 1 };
    assert_eq!(app_state.data, expected_model);
}
```

Note that we are not only testing the counter in isolation, but we are also testing
that nothing else in the data model changed. If you'd introduce a new field in the
`CounterApplication` struct, you'd have to adjust your test or otherwise it wouldn't
compile. Because of this, it is **highly** recommended that you implement the `Default` 
trait on your data model, because then creating a new field won't break your tests.
Plus, you need to initialize the data model anyways in the `main()` function.

```rust
let mut initial_state = AppState::new(CounterApplication { counter: 0, .. Default::default() });
let expected_state = CounterApplication { counter: 1, .. Default::default() };
my_button_click_handler(&mut initial_state, &mut CallbackInfo::default());
assert_eq!(app_state.data.lock().unwrap(), expected_state);
```
-->

## Summary

This chapter should have introduced you to the DOM, composing widgets via chaining functions,
callbacks and the event model and testing and maintainability of your app.