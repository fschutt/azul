## The Document Object Model (DOM)

Azul does not work like many other GUI frameworks, which can confuse newcomers. 
Other GUI frameworks allow you to create "objects", like a "Button", then 
hook up functions that are called when the button is clicked. Azul works more 
like a browser, where you can add and remove nodes in a so-called 
"Document Object Model" or "DOM" for short. Each node carries some data 
(an image, text, shape, drawing area or window) or a sub-element, which, 
in turn, is another DOM node. This create a tree-like data structure with 
parent-child relationships, often referred to as a "DOM tree".

The benefit of this approach is that it is incredibly flexible - with just
a few basic widget types, you can compose new DOM trees from existing ones,
conditionally show / hide them from the user, style them based on their
relations to each other and more, without the DOM ever "knowing" anything
about your applications data. This is good practice, since when you change
the UI of your app, you don't need to change how your app operates - the
model and the business logic stay the same while the UI changes.

## Uni-directional data flow

Azul has a "uni-directional data flow", which means that the data going
back and forth between user inputs and your application always flows in
one direction between two stages. Azul has three stages that are executed
in a loop (similar to a game engine):

1. Creating the DOM from your applications data
2. Redrawing the screen and testing if the user has interacted with the application
3. Modifying the data model according to user input, then go to step 1

![Azul Callback Model](https://i.imgur.com/cTTULrP.png)

At no point does the DOM know how the application structure looks like internally
or can modify the data behind your back. Azul views a user interface as a "view"
into your applications data and generates the whole DOM every time it needs to redraw
the screen. There are no `button.setText()` / `button.getText()` functions - the DOM
doesn't know anything about what it stores and you cannot get any information out
of the DOM once you've generated it.

## Hello World

The following code gives you a feel for what a minimal application in Azul looks like:

```rust
extern crate azul;

use azul::prelude::*;

struct MyDataModel { }

impl Layout for MyDataModel {
    fn layout(&self, _: LayoutInfo<Self>) -> Dom<Self> {
        Dom::div()
    }
}

fn main() {
    let mut app = App::new(MyDataModel { }, AppConfig::default()).unwrap();
    let window = app.create_window(WindowCreateOptions::default(), css::native()).unwrap();
    app.run(window).unwrap();
}
```

If you run this code, you should get a window like this:

![Opening a blank window](https://i.imgur.com/eY3ra97.png)

## An in-depth look

### The data model

The first step towards writing an Azul app is to declare your data model:

```rust
struct MyDataModel { }
```

This is where you store everything for your application. Yes, everything:
all database connections, all email content, passwords, user names, you name it. 
Of course, you can make sub-structs for all these details, but the `MyDataModel` 
is essentially one big object which all callbacks may have access to (we get 
into best practices with these later). In the end, your data model will probably 
look something like this:

```rust
struct MyDataModel {
    users: Vec<User>,
    app_configuration: Config,
    database_connection: Option<Connection>,
}

struct Config {
    show_sidebar: bool,
}

struct User {
    name: String,
    password: String,
    photo: Option<Icon>,
}

// and so on ... the data model can get large, but that's OK.
```

Azul itself never accesses this struct. It only needs it to hand it to the user-defined
callbacks. It wraps the data model in an `Arc<Mutex<DataModel>>`, so that it can be 
asynchronously changed on multiple threads (we'll get to asynchronous tasks later).

### The Layout trait

The `Layout` trait is the most important trait to understand. It is defined as:

```rust
pub trait Layout {
    fn layout(&self, window_info: LayoutInfo<Self>) -> Dom<Self> where Self: Sized;
}
```

The layout function has to return a `Dom<T>` where the `T` is the class you are
implementing the trait for, i.e. `Dom<MyDataModel>`. Azul uses this information to
later call functions and hand them a mutable reference to that `T`.

The `LayoutInfo<Self>` is necessary so you can return different UIs for different windows,
as well as access the context of the window (for drawing OpenGL) and immutably access
resources such as fonts and images, etc. - more on that later. You can share your data
model between windows, since the data model isn't bound to any window, information in 
one window can update the information in another window.

Note that the `layout` function takes a `&self`, not a `&mut self`: you can think of
the `layout` function like a mapping function that transforms the application state
into a user interface: `state.map(|data| data.to_dom())`.

### The DOM struct

The `Dom` struct is fairly simple: It stores a bunch of nodes that each have one 
parent node (except for the root node, which has the window itself as the parent). 
Each node has to has a type. `NodeType`s are the foundation, the building blocks of Azul,
from which all other widget can be built via composition:

- `Div` - a simple rectangle
- `Label` - holds a small string (not suited for large texts)
- `Text` - holds a `TextId` for caching larger amounts of text
- `GlTexture` - holds a callback to a rendering function that 
   yields an OpenGL texture
- `Image` - holds and image ID, kind of like `<img src="..." />` in HTML
- `Dom` - holds a callback that can generate a different DOM, given a certain
   width or height: Useful for infinite / very large data structures that can't be 
   rendered all-at-once like tables or long lists

The DOM node itself however does not only carry its data, it also carries three other
attributes:

- An optional CSS class
- An optional CSS ID
- A list of callbacks to call when the item is interacted with (empty by default)

In contrast to HTML, Azuls DOM nodes are not extensible - you can't set any
attribute to something else or add custom attributes, both for the sake of performance 
and simplicity.

All other widgets that you are going to see later simply build a DOM tree themselves,
by combining nodes or sub-dom-trees into larger widgets. Right now, the layout of 
our DOM is very simple:

```
window (dimensions: 800x600)
  └──── div (max_size: 800x600)
```

Each Dom node automatically tries to fill up the width and height of its parent,
its similar to having an implicit `display: flex; width: 100%; height: 100%` on all 
elements. Since we haven't restricted the div in width or height, it fills up the 
whole window. In the chapter about CSS we'll go over the supported CSS attributes 
and how to layout and style your app. Note that azul isn't browser-based, so there
might be differences to regular CSS, since many attributes aren't necessary in 
desktop applications. For now, we'll use the native styles returned by `css::native()`, 
which mimick the native styles of your OS - so the resulting application looks different
depending on what OS you are using.

A DOM can be built to using either the builder style (`.with_class()`, `.with_id()`),
using setters (`.add_class()`, `.set_focusable()`), using direct access to the fields
(`Dom { node_type: NodeType::Div, classes: vec!["a"], .. Default::default() }`) or by
using iterators, `my_list.iter().map(|x| Dom::label(x)).collect()` (useful for creating lists).

### Running the application

Before we can run the application, we have to do the minimal amount of setup:

1. Initialize your data model and hand it to Azul
2. Add the CSS to the app, so that Azul knows how to draw the elements
3. Open a window

```rust
let mut app = App::new(MyDataModel { }, AppConfig::default());
```

The `azul::App` stores, initializes and manages all image / fonts resources,
windows, threads and the data model for you. You will never interact with the
`App` directly, but it is still useful to know that it exists. In order to initialize
the app, you have to create the "default" state of your application, i.e. what 
state the application should be in on the first frame.

The `AppConfig` only stores things relevant to the azul framework, not your app - 
such as if logging should be enabled, etc.

```rust
let window = app.create_window(WindowCreateOptions::default(), css::native()).unwrap();
```
For multi-windowing reasons, the App also owns the renderer, so that the renderer
is shared between windows. This is why you can't call `Window::new()`, but rather
`app.create_window()`. In the WindowCreateOptions, you can set if the window
should have a certain size, decorations, transparency, fullscreen mode, etc.

Regarding the `css::native()` - azul has built-in styles for all common 
elements that are styled to look like platform-native widgets. You can 
override the default styles if you don't like them or append your own styles 
to the native styles, we'll get to that in the chapter about CSS.

```rust
app.run(window).unwrap();
```

The `run()` function starts your app and keeps running forever until either the 
last window has closed (either by the user or by the application itself) or the
application encountered a serious runtime error (which usually shouldn't happen).

### Automatic updates

Azul determines automatically when it should call the `layout()` function again. 
By default, Azul is very retained - it only repaints the screen when it's 
absolutely necessary. The process of determining which elements should be re-layouted, 
how elements should be layouted is fairly complex, but you don't need to care about it,
since the framework can handle it for you.

If you'd like to play around with the effects of auto-updating, try using 
`css::hot_reload("my_style.css")`, give your DOM node an id using the `.with_id("my_id")`
and try setting `#my_id { background-color: red; }` in the CSS file and see what 
happens if you change the color.

## Conclusion

Now that you know how Azul runs your application, we're going to build a simple
application where a user can click a button and increment a counter, to show how
to handle callbacks easily.