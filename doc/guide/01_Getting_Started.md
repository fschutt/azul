## A simple window

The code to create a simple application with an empty window looks like this:

```c
#include "azul.h"

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
}
```

or in Python, where up- and downcasting isn't necessary:

```python
from azul import *

class MyModel:
    def __init__():
        pass # empty class

def layoutFunc(data, info):
    return StyledDom.default()

inital_data = MyModel()
app = App(initial_data, AppConfig.default())
app.run(WindowCreateOptions(layoutFunc))
```



Even at this stage, Azul forces your application structure to conform to a certain style:

-   One application data model (`MyModel`)
-   A function callback that takes and renders a `MyModel` into a `StyledDom`
-   Setup code to initialize the `MyModel` and run the app

Internally, Azul runs a loop processing the input events and calls the `layoutFunc` provided to the framework in the `WindowCreateOptions` once on startup, then it caches the resulting `StyledDom`.

## Adding callbacks

While an empty window is nice to look at, it's not a user interface if the application is not interactive. So let's add a callback:

```python
def layoutFunc(data, info):
    event = EventFilter.Hover(HoverEventFilter.MouseUp)
    dom = Dom.body()
    dom.add_callback(event, data, myCallback)
    return dom.style(Css.empty())

def myCallback(data, callbackinfo):
    print("hello", flush=True)
    return Update.DoNothing
```

Now the console will print "hello" if you click anywhere on the window. By default, the `body` node type is expanded to its maximum size (so it covers the entire window).

In this case the callback returns `Update.DoNothing`, which to Azul signifies that nothing in the UI has changed and calling the `layoutFunc` again is unnecessary.

If any callbacks change the UI, they need to return `Update.RefreshDom`, which will trigger Azul to call `layoutFunc` again. Since `Update.RefreshDom` is invoked infrequently, the `layoutFunc` only gets called a few times per second at most: fast enough to be considered "reactive", but not fast enough to stress the users CPU.

It is important to note that the callbacks have no direct access to the UI objects or the UI hierarchy. All changes that modify the DOM hierarchy must be done via the data model (there are some exceptions for style-only changes, animations and changes to the text content of a node).



## The data model

```python
class DataModel:
    def __init__():
        pass
```

This is where you store application-relevant data: database connections, email content, passwords, user names, you name it. The model is custom to the application that you are building and in the end it will probably look like this:

```python
class MyDataModel:
    def __init__():
        self.users = [
            User("Anne", "Shirley", photo=None),
            User("Matthew", "Cuthbert", photo="/img/users/Matthew.png")
        ]
        self.app_config = AppConfig()
        self.database_connection = None
        # ... etc.
```

Azul itself never accesses this struct. It only needs it to hand it to the user-defined callbacks. It wraps the data model in an `RefAny` struct which is then "cloned" onto the callback (so that the callback has mutable access to the application data via the `data` field in the callback). "Cloning" a `RefAny` performs a shallow clone: The data is reference-counted, the actual data only exists once. But be careful: Modifying the data will change it for all callbacks that have a reference to the data.

<br/>

## Layout

The layout function that needs to be passed to the `WindowCreateOptions` is defined as:

```rust
fn layout(data: &RefAny, info: LayoutInfo) -> StyledDom
```

You can construct a `StyledDom` either directly (via `default()`) or by combining a `Dom` with a `Css` object:

```python
# DOM + CSS = StyledDom
def layout(data, info):

    a = StyledDom.default()
    b = Dom.body().style(Css.empty()) # equivalent

    # if the CSS contains a syntax error, will return Css.empty()
    css1 = Css.from_string("div { background: red; }")
    c = Dom.div().style(css1)

    # css1 does not affect this DOM: CSS is local, not global
    css2 = Css.from_string("body { background: green; }")
    d = Dom.from_xml("<body><p>Hello</p></body>").style(css2)

    e = Dom.body().with_child(Dom.text("Hello"))
    if layout.dark_mode:
        # Azuls File.open will automatically close the file
        # when it goes out of scope
        #
        # @throws Exception if the file could not be read
        e = e.style(File.open("dark.css").read_to_string())
    else:
        e = e.style(File.open("light.css").read_to_string())

    a.add_child(a)
    a.add_child(b.with_child(c))

    return a
```

<br/>

There are six types of DOM nodes:

<br/>

1.  `Body` Should only be used on the root node, same as div, but automatically expands to the maximum width / height
2.  `Div` Rectangular box
3.  `Text(String)` Contains a text string
4.  `Br` Signifies a line break between two text strings. By default all text strings will be laid out contiguously.
5.  `Image(ImageRef)` Contains an image (decoded image bytes or OpenGL texture ID)
6.  `IFrame(IFrameCallback)` Contains an iframe, i.e. a callback that - when being called given the size of the parent node - returns a DOM again: useful to implement infinite-scrolling

<br/>

All other widgets that you are going to see later simply build a DOM tree (`Button`, `Label`), by combining nodes or sub-dom-trees into larger widgets.

<br/>

## The callback

When a user-provided event satisfies the `EventFilter.Hover(HoverEventFilter.OnMouseUp)` filter, Azul will call the provided callback with the `RefAny` that was submitted along with it. In this case, it is a mutable reference to the entire application data.

```rust
fn callback(data: &mut RefAny, info: CallbackInfo) -> Update
```

The callback type takes a mutable reference to the `RefAny` and an additional `info` struct containing many useful functions:

```python
def callback(data, info):

    # in Rust you'd need to downcast_mut() to reference your data mutably
    data.users.append(User("Marilla", "Cuthbert", photo=None))

    # CallbackInfo contains many useful functions
    hit_node_id = info.get_hit_node()
    parent_node_size = info.get_node_size(hit_node_id)

    # toggle window flags
    keyboard_state = info.get_keyboard_state()
    window_flags = info.get_window_flags()
    if keyboard_state.current_keys.contains(VirtualKeyCode.F11):
        window_flags.is_fullscreen = not window_flags.is_fullscreen
        info.set_window_flags(window_flags)
    elif keyboard_state.current_keys.contains(VirtualKeyCode.Esc):
        window_flags.is_about_to_close = True # close window
        info.set_window_flags(window_flags)

    # get the parent size
    hit_node_parent = info.get_parent(hit_node_id)
    if hit_node_parent is not None:
        parent_node_size = info.get_node_size(parent_node_id)

    # perform a hit-test on a text node
    inline_text = info.get_inline_text(hit_node_id)
    if inline_text is None:
        return Update.DoNothing # error

    cursor = info.get_cursor_relative_to_item()
    hits = inline_text.hit_test(cursor)

    hit = None

    for hit in hits:
        line = hit.line_index_relative_to_text
        col = hit.char_index_relative_to_line
        print("clicked on line " + line + " character " + col + " ")
        hit = hit.hit_relative_to_inline_text

    # modify a CSS property
    cursor_node_id = hit_node_id.get_first_child()
    if cursor_node_id is None:
        return Update.DoNothing

    info.set_css_property(cursor_node_id, CssProperty.Transform([
        Transform.Translate(PixelValue.px(hit.x), PixelValue.px(hit.y))
    ]))

    # timer_1_id = info.start_animation(hit_node_id, Animation(...))
    # timer_2_id = info.start_timer(data, myTimerCallback)
    # info.stop_timer(timer_2_id)
    #
    # thread_id = info.start_thread(data, myThreadCallback, onFinishCallback)
    # thread_id = info.stop_thread(thread_id)
    #
    # info.open_window(WindowCreateOptions.new(myOtherLayoutCallback))
    #
    # etc. - see API reference

    return Update.DoNothing # setting CSS properties does not require re-layout
```

<br/>

## Running the application

Before we can run the application, we have to do the minimal amount of setup:

<br/>

1.  Initialize your data model and hand it to Azul
2.  Initialize the `App` with a given `LayoutSolver`
3.  Run the app in a root window described by the `WindowCreateOptions`

<br/>

The reason specifying the layout solver is required is because there might be multiple layout solvers in the future or you may need to work around specific layout bugs on specific versions. In order to keep Azul forward-compatible, this versioned-layout model approach ensures that your layout will never break even with future versions of Azul.

The `WindowCreateOptions` contains fields that you can configure before handing the object to Azul:

```python
window = WindowCreateOptions.new(layoutFunc)

# use software rendering
window.renderer_type = RenderType.Software

# start window maximized
window.state.flags.is_maximized = True

# call the layoutFunc every 200ms:
# combined with Dom.from_xml(File.open("ui.xml").read_to_string())
# you can design your UI at runtime
window.hot_reload = True

# Set the title of the window
window.state.title = "MyApp"
```

<br/>

The `App` stores, initializes and manages all images / fonts resources, windows, threads and the data model for you. You will never interact with the `App` directly, but it is still useful to know that it exists. If all windows are closed the `run()` method finishes.

<br/>

## Conclusion

Azul caches as much as possible about your UI: the `StyledDom`, the computed layout, CSS properties, calculated styles, positions and sizes, etc.

-   If the DOM hierarchy does not change it is preferred to use the `callbackinfo.set_css_property()` methods to change the CSS.
-   Changes to `opacity` and `transform` properties are GPU-accelerated, meaning they do not require Azul to re-generate a display list.
-   Changing style properties (colors, gradients, images, etc.) requires a new display list, but does not require recomputing the cached layout.
-   Changing layout properties (such as `width` or `height`) changes the cached layout, but does not require a call to `myLayoutFunc` or any CSS re-styling.
-   Only if you need to re-generate the entire UI, return `Update.RefreshDom` from the callback.

Now that you know how Azul runs your application, you should be able to read the simple counter example:

```python
from azul import *

css = """
    .__azul-native-label { font-size: 50px; }
"""

class DataModel:
    def __init__(self, counter):
        self.counter = counter

# model -> view
def my_layout_func(data, info):
    label = Label("{}".format(data.counter))
    button = Button("Update counter")
    button.set_on_click(data, my_on_click)

    dom = Dom.body()
    dom.add_child(label.dom())
    dom.add_child(button.dom())

    return dom.style(Css.from_string(css))

# model <- view
def my_on_click(data, info):
    data.counter += 1;

    # tell azul to call the my_layout_func again
    return Update.RefreshDom

model = DataModel(5)
app = App(model, AppConfig(LayoutSolver.Default))
app.run(WindowCreateOptions(my_layout_func))
```

