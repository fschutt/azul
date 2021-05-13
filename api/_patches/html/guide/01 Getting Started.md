<h2>A simple window</h2>

  <p>The code to create a simple application with an empty window looks like this:</p>

<code class="expand">#include "azul.h"

typedef { } struct MyModel;
void MyModel_destructor(MyModel* instance) { }
AZ_REFLECT(MyModel, MyModel_destructor);

AzStyledDom layoutFunc(AzRefAny* data, AzLayoutCallbackInfo info) {
    return AzStyledDom_default();
}

int main() {
    AzRefAny initial_data = MyModel_newRefAny(MyModel { });
    AzApp app = AzApp_new(initial_data, AzAppConfig_default());
    AzApp_run(app, AzWindowCreateOptions_new(layoutFunc));
    return 0;
}</code>

<p>or in Python, where up-and downcasting isn't necessary:</p>

<code class="expand">from azul import *

class MyModel:
    def __init__():
        pass # empty class

def layoutFunc(data, info):
    return StyledDom.default()

inital_data = MyModel()
app = App(initial_data, AppConfig.default())
app.run(WindowCreateOptions(layoutFunc))</code>


<p>
Azul forces your application structure to conform to a certain style:
</p>

<br/>
<ul>
    <li><p>&nbsp;&nbsp;One application data model (<code>MyModel</code>)</p></li>
    <li><p>&nbsp;&nbsp;A function callback that takes an renders a <code>MyModel</code> into a <code>StyledDom</code></p></li>
    <li><p>&nbsp;&nbsp;Setup code to intialize the <code>MyModel</code> and run the app</code></p></li>
</ul>

<p>
    Internally, Azul runs a loop processing the input events
    and calls the <code>layoutFunc</code> provided to the framework
    in the <code>WindowCreateOptions</code> once on startup, then
    it caches the resulting <code>StyledDom</code>.
</p>
<p>
    If any callbacks change the UI, they return
    <code>Update.RefreshDom</code>, which will trigger Azul to call
    <code>layoutFunc</code> again. Since <code>Update.RefreshDom</code> is
    invoked infrequently, the <code>layoutFunc</code> only gets called a few
    times per second at most: fast enough to be considered "reactive",
    but not fast enough to stress the users CPU.
</p>

<br/>
<h2>The Why of Azul</h2>

<p>
    In order to understand why Azul forces this model, you have to understand the problem with nearly
    every other toolkit. In other toolkits, your app would look roughly like this:</p>

<code class="expand">class MyApp extends othertoolkit.App:
    self.button = Button(...)
    self.text_input = TextInput(...)
    self.output = Label(...)

    self.add(self.button)
    self.add(self.text_input)
    self.add(self.output)

    @override
    def on_click():
        # we somehow need to access the text input
        input = self.text_input.getText()
        calculated = do_somthing_with_input(input)
        self.output.setText(calculated)
        self.text_input.setText("")
</code>

<p>This example is simple, but there are already a couple of problems:</p>
<br/>
<ol>
    <li><p>The application data has to store UI objects in order to access them from callbacks: You do not care about the UI objects, you only care about the input data, but you have to carry them around in order to get access to the <code>text_input.getText()</code></p></li>
    <br/>
    <li><p>You need to remember to reset the text input after calculation and update the UI. Forget one <code>setText</code> call and the UI will be out of sync. Other functions might expect the UI to be in a certain state (i.e. the text input is expected to be empty before calling function <code>foo()</code>)</p></li>
    <br/>
    <li><p>Removing and exchanging UI objects on state changes becomes very hard.
        Some frameworks auto-delete children objects, which means you need to make sure that no other objects are holding references to <code>self.text_input</code></p></li>
    <br/>
    <li><p>The state is not easily testable because there are a lot of stateful calls changing
    your UI objects. Other classes might unexpectedly change UI objecs in their parent classes -
    not to speak of multithreading</p></li>
    <br/>
    <li><p>Layout and hierarchies become a pain, even using "layout managers", there is little room for exchanging layout managers (i.e. switching from a horizontal layout to a vertical layout one at runtime)</p></li>
    <br/>
    <li><p>Your inheritance hierarchy is bound to the visual hierarchy of objects the screen. This works for linear hierarchies (i.e. <code>SideBarPanel extends Sidebar extends App</code>), but not for non-linear ones (SideBar has to interact with MainTable). Redesigning your App will force you to refactor inheritance hierarchies, which takes time.</p></li>
    <br/>
    <li><p>Most of the code is run on startup, leading to both bad startup time and less flexibility when for example switching screens in a single-page application</p></li>
    <br/>
    <li><p>Because of the inheritance and because you need to store UI objects inside of your class, you've created a hard dependency on the toolkit. Should you ever want to migrate away from the toolkit, it will be painful since your code could depend on functionality inherited from the GUI toolkit.</p></li>
    <br/>
    <li><p>Callbacks can only access data from the <code>MyApp</code> class, not from any other UI classes. If multiple, unrelated UI objects need to talk to each other,
        you need superclasses or references to the least common "parent object" in an inheritance hierarchy</p></li>
</ol>

    <p>
        etc. etc. Yet for some reason, programmers are almost addicted
        to this style of programming, likely by force, since the only
        mature GUI toolkits so far tried to just copy Java. Java was designed
        in an environment where CPU clock speed was still measured in megahertz,
        so it makes perfect sense to optimize for less computation.
    </p>

    <p>
        But wait!, you say. There's MVVM and MVC and all those other acronyms I've heard about.
        Yes. Every few years there's a new methodology in town on how to do object-oriented GUI
        programming "for real this time". Yet somehow, it never works: Nobody agrees on how to
        actually implement "MVC": If you ask ten programmers what a "controller" or a "view model" is
        - or even what a "model" is, you will get twenty different answers. And even then, MVC or MVVM
        do not solve the three root problems:
    </p>

    <br/>
    <ol>
        <li><p><strong>Data Access / Model-View separation: </strong> Somehow the callback needs access to both the data model (i.e. the class) and the stateful UI object (to scrape the text out), but at the same time the "model" should be separate from the UI so that logic functions do not depend on view data.</p></li>
        <li><p><strong>Synchronization:</strong> It is very easy for the visual UI state and the data model to go out of sync. Solutions are "Observer patterns" (callbacks that run when something changes), React-like reconciliation or "just redraw everything" (IMGUI)</p></li>
        <li><p><strong>Inter-widget communication: </strong> Most toolkits assume that the widget hierarchy and the inheritance (or function call) hierarchy are the same. If two UI objects that have no common parent have to talk to each other, you now have a massive problem.</p></li>
    </ol>

    <p>
        Solutions such as Immediate Mode GUI "solve" this problem by hiding the
        data binding to the callback in a closure-with-captured-arguments instead
        of a class-with-state-and-functions: The form is different, but the operation is the same.
    </p>
    <p>
        A closure is quite literally just an anonymous (non-nameable) function on an
        anonymous struct (which contains all captured variables).
        So the effect is the same and on top of that, it provides even less layout
        flexibility and has the

        Immediate Mode GUI solves the synchronization problem, but it fails at
        the other two problems. It doesn't solve the problem and the
        performance tradeoffs are - usually - immense.
    </p>

    <br/>
    <h2>UI trees and function call stacks</h2>

    <p>
        Some people say that HTML is not a programming language.
        However, if you look at the origins of HTML, you can see the
        similarities between HTML (or general UI hierarchy tree models)
        and function call stacks - compare:
    <p>

<code>&lt;div class="parent"&gt;
    &lt;div class="child"&gt;&lt;/div&gt;
    &lt;div class="child"&gt;&lt;/div&gt;
    &lt;div class="child"&gt;&lt;/div&gt;
&lt;/div&gt;</code>

    <p>with:</p>

<code>div(class="parent", children = [
    div(class="child")
    div(class="child")
    div(class="child")
])</code>

    <p>
        Composing UI hierarchies via functions makes much more
        sense than composing UI hierarchies via inheritance
        because the latter is often language-specific and not
        supported in all languages, whereas functions are language
        agnostic.
    </p>

    <p>

    </p>

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
</p>