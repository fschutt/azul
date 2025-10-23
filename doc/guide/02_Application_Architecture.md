# Towards A Perfect GUI Toolkit

For forty years, building graphical user interfaces has been one of the most persistently difficult 
problems in software engineering. Despite a constant evolution of languages, libraries, and design 
patterns, developers still struggle with the same fundamental challenges: managing state, synchronizing 
data with the view, and enabling communication between distant components. 

The root of this struggle lies in a core conflict that nearly every toolkit fails to properly address: 
the conflict between the **Visual Tree** and the **State Graph**.

*   The **Visual Tree** is the hierarchy of elements as they appear on the screen. It is always a
    tree: a window contains a panel, which contains a button. Its structure is defined by layout and presentation.
*   The **State Graph** is the map of how application data and logic are connected. A filter control
    in a sidebar (`Visual Tree` -> `Sidebar` -> `Filter`) needs to alter the data displayed in a completely
    separate table  (`Visual Tree` -> `MainPanel` -> `Table`). A "Save" button in a toolbar must know if form data,
    located elsewhere, is valid. This network of dependencies is a complex **graph**, not a simple **tree**.

The history of GUI development is the history of failed or incomplete attempts to reconcile these 
two different structures. The "pain" of UI programming stems from frameworks that either fuse them 
together or awkwardly force the graph to conform to the shape of the tree.

## First Gen: Fused Hierarchy (OOP)

The first generation of toolkits (Qt, GTK, MFC, Swing) was built on an object-oriented model. The 
paradigm was simple: the UI is a tree of stateful objects. A `Button` object holds its own text and 
state, a `MyCustomPanel` object inherits from `Panel` and adds its own data and logic.

```python
# OOP Paradigm Model
class MyApp(othertoolkit.App):
    # ...
    def on_click():
        input = self.text_input.getText()
        calculated = do_somthing_with_input(input)
        self.output.setText(calculated)
        self.text_input.setText("")
```

In this model, the Visual Tree and the State Graph are **fused**. The object inheritance hierarchy *is* 
the visual hierarchy. This immediately creates real problems:

*   Communication between logically related but visually distant components requires complex pointer
    management, global mediator objects, or a web of signal-and-slot connections that are difficult to
    trace and maintain.
*   **Changing the visual layout in this paradigm forces a refactoring of the class hierarchy**, which
    makes developing applications in such toolkits painful and creates hard dependencies on the toolkit
    itself (leading to online "toolkit wars", GTK vs Qt). The application logic is not testable in isolation
    because it is fundamentally inseparable from the UI objects themselves.
*   It creates a hard dependency on the toolkit itself. Your application logic is not portable
    or reusable because it is fundamentally intertwined with the toolkit's base classes, rendering
    system, and event model.

## Second Gen: Constrained Hierarchy (React)

The next major step, led by frameworks like React and the Elm Architecture, introduced a new 
functional paradigm: **`UI = f(data)`**. The UI is a declarative, pure function of the application's 
state. This was revolutionary at the time, as it solved the problem of state synchronization. The 
developer no longer manually calls `setText()`; he changes the data, and the framework efficiently 
updates the view to match.

```python
# React Paradigm Model
def MyApp():
    input_value, set_input_value = useState("")
    output_value, set_output_value = useState("")

    def handle_click():
        calculated = do_something_with_input(input_value)
        set_output_value(calculated)
        set_input_value("")

    return Page(children=[
        TextInput(value=input_value, on_change=set_input_value),
        Button(on_click=handle_click),
        Label(text=output_value)
    ])
```

However, while these frameworks finally decouple the view from imperative manipulation, they 
still, by default, **constrain the flow of data to the shape of the Visual Tree**. The example 
above works because `TextInput`, `Button`, and `Label` are all siblings, children of `MyApp`.

But what if the `Button` were in a `Toolbar` and the `TextInput` and `Label` were in a `MainContent` 
panel? React's solution is to "lift state up" to their lowest common ancestor, `MyApp`. The `MyApp` 
component must now hold the state and pass both the data and the callback functions down through the 
intermediate components.

```python
def MyApp():
    # State is lifted to the common ancestor
    input_value, set_input_value = useState("")

    # ... logic also lives in the ancestor ...

    return Page(children=[
        # Toolbar is now forced to accept and pass down a prop it doesn't use
        Toolbar(on_button_click=handle_click),
        # MainContent is also forced to pass props
        MainContent(
            input_value=input_value,
            on_input_change=set_input_value,
            output_value=output_value
        )
    ])
```

The State Graph is still being forced into the tree structure of the view, leading to "prop drilling" 
and components with bloated, indirect APIs. The existence of complex "escape hatches" like Redux or 
the Context API is evidence of this core constraint—they are patterns invented to work *around* this 
default tree-based data flow.

## Third Gen: Ignoring Hierarchy (IMGUI)

Immediate Mode toolkits (IMGUI) take a different approach. The paradigm is to have no persistent UI objects 
at all; the UI is redrawn from scratch from application data every single frame. This solves synchronization 
by brute force (but ignores all other problems, as React did).

```python
# IMGUI Paradigm Model
class AppState:
    input_buffer = ""
    output_text = ""

# Inside the main application loop, every frame
def render_ui(app_state):
    ui.text_input("Input:", &app_state.input_buffer)
    if ui.button("Calculate"):
        calculated = do_something_with_input(&app_state.input_buffer)
        app_state.output_text = calculated
        app_state.input_buffer.clear()
    ui.label(&app_state.output_text)```
```

However, IMGUI doesn't solve the Visual Tree vs. State Graph problem—it largely **ignores it**. Logic 
and rendering are mixed in a single, procedural pass. In the example, the calculation logic is executed 
directly inside the rendering

## Why Electron Won

The success of Electron is a direct consequence of this architectural vacuum. In the 2010s, developers 
were flocking to the declarative web paradigm because it was demonstrably more productive and maintainable 
than the 1990s-era OOP model. When tasked with building a desktop application, they had a choice: revert 
to the painful, fused hierarchy paradigm of Qt or GTK, or leverage the modern, constrained hierarchy of React.

Electron provided the bridge. While many developers were surely unconscious about it, they chose it 
not for its performance (or lack of it), but for its better paradigm. The native desktop world had 
no compelling answer to the superior React-ive paradigm, so developers chose the superior architecture, 
accepting the performance cost and tons of build-tool workarounds as a necessary evil.

Azul, however, is not an answer from the second era. It is a "Fourth Generation" model. It acknowledges 
the declarative revolution of `UI = f(data)` but also recognizes the architectural limitations of the 
constrained hierarchy.

## Quid est: GUI?

So, what is a "GUI toolkit"? How does it differ from just a rendering library?

As shown above, one can mainly categorize the toolkit by its handling of the following three "hard GUI problems":

1.  **Data Access / Model-View separation:** Somehow a callback needs access to both the data model (i.e. the class)
    and the stateful UI object (to scrape the text out), but at the same time the "data model" should be as separate
    from the UI as possible, so that logic functions do not depend on view data.

3.  **Synchronization:** It is very easy for the visual UI state and the data model to go out of sync.
    Solutions so far include "Observer patterns" (callbacks that run when something changes), React-like
    reconciliation or "just redraw everything" (IMGUI).

4.  **Inter-widget communication:** Existing toolkits assume that the widget hierarchy (visual tree) and
    the inheritance (or function call) hierarchy are the same (least common ancestor problem). If two UI
    objects that have no common parent have to talk to each other, you now have a massive problem.

Pure "rendering libraries" do not solve these problems at all, instead shoving the responsibility onto 
the application programmer (aka. "not my job"). The result of such "freedom" to design any application 
style is often sub-par, but enjoys a large popularity because it secures the job of whoever first wrote 
the application.

## Starting again

So what would a "proper" toolkit look like?

The first thing we'd need to decide is some way to 












As computers got faster and rendering methods evolved, 
both browsers and UI toolkits moved away from a direct render-to-screen to a display-list or a "list 
of commands" for the renderer. Often times this command list is batched or computed against the last 
frame to minimize changes - while there is a small overhead, it is almost unnoticeable.

The next problem is that widget-specific data has to be either stored on the programmer side (in the application, using inheritance or traits) or in the framework (either using data attributes or - worse - global state modifying functions such as synchronous `setColor(RED); draw(); swap();` calls).

So the only real "sane" way is to serialize the entire UI hierarchy and then perform layout, state and cache analysis in the toolkit. A good comparison is to compare XML to function call stacks - compare:

```xml
<div class="parent">
    <div class="child"></div>
    <div class="child"></div>
    <div class="child"></div>
</div>
```

with:

```python
div(class="parent", children = [
    div(class="child")
    div(class="child")
    div(class="child")
])
```

Composing UI hierarchies via functions makes much more sense than composing UI hierarchies via inheritance because the latter is often language-specific and not supported in all languages, whereas functions are language agnostic. Additionally you can render UI nodes in parallel:

```rust
let children = data_collection
    .par_iter()
    .map(|object| render_ui(object))
    .collect();

return parent.with_children(children);
```

## Data access: A question of format and locality

Great, so we need to serialize the UI into a tree structure. But where do we store our widget data? And in what format?

Inheritance-based toolkits only allow one format: You have to inherit from a UI object and then construct your application as a series of UI objects. Azul however, stores the application data as an implementation-agnostic `RefAny` struct: similar to `PyObject` or Javascripts `Object` it just stores "some data", but the toolkit doesn't know what the type is. You can upcast your data and wrap it in a `RefAny` via `RefAny::new` and then get immutable or mutable access again via `.downcast_ref()` or `.downcast_mut()`, respectively:

```rust
let data = RefAny::new(5);
let data_ref = data.downcast_ref::<usize>();
println!("{}", data); // prints "5"
```

In Python, this process is transparent to the user, since every Python object is already wrapped in an opaque, reference-counted type, so it would look like this:

```python
data = RefAny(5); # RefAny<PyObject<PyInteger>>
print(str(data._py_data)) # prints "5"
```

While Rust only knows about the `RefAny`, the Python bindings automatically perform the front-and-back conversion from python objects. Both the object type and immutable / mutable reference count is checked at runtime: When all references to a `RefAny` are deleted, the internal object is deleted, too.

Azul can store `RefAny` data in three locations:

1.  **Callback-local data:** Data is stored directly on the callback: other callbacks cannot access this.
2.  **Widget-local data:** Stored on the UI XML nodes as a `dataset` property: Callbacks can have shared access to this data via `callback.get_dataset(node_id)`: but only if they know the `NodeId` that the dataset is attached to.
3.  **Application-global data:** Stored inside the `App` struct: all layout callbacks are rendered from this data.

A useful property is that `RefAny` structs can contain both callbacks and `RefAny` types themselves (nested `RefAny`). This way you can implement user-provided functionality by simply creating some fields and wrapper-methods that "wrap" a user-provided `RefAny` and a callback to be called when your custom event is ready:

```python
class MyWidget:
    def __init__():
        self.on_text_clicked = None

    # Allow users to set a custom event
    def set_on_text_clicked(data, callback):
        self.on_text_clicked = tuple(data, callback)

    def dom():
        dom = Dom.text("hello")
        if self.on_text_clicked is not None:
            dom.add_callback(On.MouseUp, self.on_text_clicked, _handle_event)
        return dom

# Note that data is the tuple(data, callback):
# you can use custom classes in order to customize callbacks
# in total, this provides more flexibility than @override
def _handle_event(data, callbackinfo):

    # "data" contains the **user-provided** callback
    # and a RefAny to **user-provided** data
    #
    # In this example, we you can now choose to
    # invoke or not invoke the callback depending on whether
    # the text of the object was hit

    text = callbackinfo.get_inline_text(callbackinfo.get_hit_node())
    if text is None:
        return Update.DoNothing

    text_hit = None
    hits = text.hit_test(info.get_hit_relative_to_item())
    if len(text_hits) != 0:
        text_hit = hits[0]
    else:
        return Update.DoNothing

    # do some default processing here
    print("before invoking user callback: text was hit")

    # invoke the user-provided function with additional information
    result = (data[1])(data[0], callbackinfo, text_hit)

    print("after invoking user callback: user returned" + str(result))

    # return the result of the user-provided callback to Azul
    return result
```

The important part is that you can now customize the callback type (including the return type) and invoke the callback with extra data in order to do pre- and post-processing of user callbacks.

## Backreferences

The programming pattern that naturally emerges from this architecture are "backreferences" or: the lower-level widget (`Dom.div`, `TextInput`, etc.) can take optional function pointers and data references to higher-level widgets (such as `NumberInput`, `EMailInput`), which then perform validation for you.

```python
class TextInput:
    text: String
    # "backreference" to user-provided callback + data
    user_focus_lost_callback: Optional[tuple(data, callback)]

    def __init__(text):
        self.text = text
        self.user_focus_lost_callback = None

    def set_on_focus_lost(data, callback):
        self.user_focus_lost_callback = tuple(data, callback)

    def dom():
        dom = Dom.text(self.text)
        refany = RefAny(self)
        dom.set_callback(On.TextInput, refany, _on_text_input)
        dom.add_callback(On.FocusLost, refany, _on_focus_lost)
        return dom

# callback knows that "data" is of type TextInput
def _on_text_input(data, callbackinfo):
    data.text += callbackinfo.get_keyboard_input().current_char
    callbackinfo.set_text_contents(callbackinfo.get_hit_node_id(), data.text)
    return Update.DoNothing

def _on_focus_lost(data, callbackinfo):
    user_cb = None
    if data.user_focus_lost_callback is None:
        return Update.DoNothing
    else:
        user_cb = data.user_focus_lost_callback

    # invoke the user-provided callback with the extra information
    return (user_cb[1])(user_cb[0], self.string)
```

```python
class NumberInput:
    number: Integer
    # "backreference" to user-provided callback + data
    on_number_input: Optional[tuple(data, callback)]

    def __init__(number):
        self.number = number
        self.on_number_input = None

    def set_on_number_input(data, callback):
        self.on_number_input = tuple(data, callback)

    def dom():
        ti = TextInput("{}".format(self.number))
        ti.set_on_focus_lost(RefAny(self), _validate_text_input_as_number)
        return

def _validate_text_input_as_number(data, callbackinfo, string):
    if data.on_number_input is None:
        return Update.DoNothing

    number = string_to_number(string)
    if number is None:
        return Update.DoNothing # input string is not a number

    # string has now been validated as a number
    # optionally can also validate min / max input, etc.
    (data.on_number_input[1])(data.on_number_input[0], callbackinfo, number)
```

```python
class MyApplication:
    user_age: None

    def __init__(initial_age):
        self.user_age = initial_age

def layoutFunc(data, layoutinfo):
    ni = NumberInput(initial_age)
    ni.set_on_number_input(data, _on_age_input)
    return ni.dom().style(Css.empty())

# will only get called when the text input is a valid number
def _on_age_input(data, callbackinfo, new_age):
    if new_age < 18:
        MsgBox.ok("You must be older than 18 to proceed")
        return Update.DoNothing
    else:
        data.user_age = new_age
        return Update.RefreshDom
```

```python
app = App(MyApplication(18), AppConfig(LayoutSolver.Default))
app.run(WindowCreateOptions(layoutFunc))
```

For the sake of brevity the code uses `RefAny(self)` to create new `RefAny` references. All built-in widgets use extra classes such as `TextInputOnClickedData`, `TextInputOnFocusLostData` to separate the `TextInput` from the data it has to carry even further.

When the user presses the "TAB" or "shift + TAB" key, the current field loses focus and thereby triggers an `On.FocusLost` and `On.FocusReceived` event. If it isn't already clear from reading the code, here is how the event "travels" through the classes (from the low-level `on_focus_lost` to the high-level `on_age_input`):

```
1. _on_focus_lost(&mut RefAny<TextInput>)
2. _validate_text_input_as_number(&mut RefAny<NumberInput>, string)
3. _on_age_input(&mut RefAny<MyApplication>, number)
```

The closer you get to the application, the more custom your function callback types will be. Note that the only "state" that gets passed into callbacks is via function arguments. Functional programmers can see this as an "application" of function parameters: the actual core state management is hidden completely.

## Non-linear widget hierarchies

Let's take the example of a node graph: Draggable nodes can be added and removed, contain text, number and color inputs, check- and radio boxes, contain inputs and outputs (which need to validate their input and output types when they are connected).

Node graphs are pretty much a litmus test to how flexible your UI toolkit is: In any object-oriented toolkit you will get problems designing the hierarchy here.

![Node graph example](https://docs.unrealengine.com/Images/ProgrammingAndScripting/Blueprints/UserGuide/Nodes/SelectNode.jpg)

Azul includes a built-in `NodeGraph` as a default widget, so let's take a look at the implementation.

The core of the widget is the `NodeGraph` struct. This is a plain data structure; it holds `Vec`s of node types, node instances, and connections. It is the single source of truth for the entire state of the graph. It contains no UI objects.

```rust
// nodegraph.rs

#[derive(Debug, Clone)]
#[repr(C)]
pub struct NodeGraph {
    pub node_types: NodeTypeIdInfoMapVec,
    pub input_output_types: InputOutputTypeIdInfoMapVec,
    pub nodes: NodeIdNodeMapVec, // A vector of Node structs
    pub allow_multiple_root_nodes: bool,
    pub offset: LogicalPosition,
    // ... other plain data fields
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Node {
    pub node_type: NodeTypeId,
    pub position: NodePosition,
    pub fields: NodeTypeFieldVec,
    pub connect_in: InputConnectionVec,   // A vector of input connections
    pub connect_out: OutputConnectionVec, // A vector of output connections
}
```

The `dom()` function for the `NodeGraph` is a pure function that takes this data and transforms 
it into a descriptive tree of `Dom` nodes. It iterates through `self.nodes` and calls `render_node()` 
for each, and then iterates through the connections and calls `render_connections()`. The UI is a 
predictable result of the data.

```rust
// nodegraph.rs

impl NodeGraph {
    // Takes ownership of the data `self` and returns a UI description `Dom`
    pub fn dom(self) -> Dom {
        // ...
        Dom::div()
            // ...
            .with_children({
                vec![
                    // connections
                    render_connections(&self, node_connection_marker),
                    // nodes
                    self.nodes
                        .iter() // Iterates over the node data
                        .filter_map(|NodeIdNodeMap { node_id, node }| {
                            // ... find node_type_info
                            Some(render_node( // Calls render_node() for each
                                node,
                                (self.offset.x, self.offset.y),
                                &node_type_info.node_type_info,
                                node_local_dataset,
                                self.scale_factor,
                            ))
                        })
                        .collect::<Dom>()
                    // ...
                ]
                .into()
            })
            // ...
    }
}
```

So, how does interaction work? Consider what happens when a user clicks on an output port of one node and an input port of another.

1.  Each input and output port is rendered with a callback attached. This callback is created with a `NodeInputOutputLocalDataset`.

```rust
// nodegraph.rs -> inside render_node() function

// ... iterating through inputs ...
    Dom::div()
        .with_callbacks(vec![
            CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::LeftMouseUp),
                // The dataset and callback are attached here
                data: RefAny::new(NodeInputOutputLocalDataset {
                    io_id: Input(io_id),
                    backref: node_local_dataset.clone(), // <- IMPORTANT
                }),
                callback: CoreCallback { cb: nodegraph_input_output_connect as usize },
            },
            // ... other callbacks
        ].into())
    // ...
```

2.  Crucially, this dataset contains a **backreference** (`RefAny`) to its (UI-logical) **parent** `NodeLocalDataset`.

```rust
// nodegraph.rs

struct NodeInputOutputLocalDataset {
    io_id: InputOrOutput,
    backref: RefAny, // RefAny<NodeLocalDataset>
}
```

3.  The `NodeLocalDataset`, in turn, contains a backreference to the top-level `NodeGraphLocalDataset`, which holds the entire `NodeGraph` state.

```rust
// nodegraph.rs

struct NodeLocalDataset {
    node_id: NodeGraphNodeId,
    backref: RefAny, // RefAny<NodeGraphLocalDataset>
}

struct NodeGraphLocalDataset {
    node_graph: NodeGraph, // The entire state is held here
    last_input_or_output_clicked: Option<(NodeGraphNodeId, InputOrOutput)>,
    // ... other fields
    callbacks: NodeGraphCallbacks,
}
```

When the callback for the second click fires (`nodegraph_input_output_connect`), it uses this chain of backreferences to instantly access the top-level state. It does not traverse a visual tree or send messages up to a parent. It makes a direct jump from the event source (the input port) to the relevant data model (the `NodeGraph` struct) and invokes the logic to create a connection (`backref.node_graph.connect_input_output(...)`).

```rust
// nodegraph.rs

extern "C" fn nodegraph_input_output_connect(data: &mut RefAny, info: &mut CallbackInfo) -> Update {

    // Step 1: Downcast to the immediate dataset
    let mut data = match data.downcast_mut::<NodeInputOutputLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    // Step 2: Use the first backreference to get the node-level dataset
    let mut backref = match data.backref.downcast_mut::<NodeLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    // Step 3: Use the second backreference to get the top-level graph dataset
    let mut backref = match backref.backref.downcast_mut::<NodeGraphLocalDataset>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    // Now `backref` holds the entire NodeGraph state, accessible directly.
    let (input_node, input_index, output_node, output_index) =
        match backref.last_input_or_output_clicked.clone() {
            // ... logic to determine which nodes to connect ...
        };

    // Direct modification of the data model
    match backref.node_graph.connect_input_output(
        input_node,
        input_index,
        output_node,
        output_index,
    ) {
        // ... handle result ...
    }
    // ...
}
```

This modification of the central `NodeGraph` data struct then triggers a UI refresh, and 
the `dom()` function re-renders the new connection line on the screen. The flow 
is: **Event -> Direct State Modification -> Declarative Re-render**.

```rust
// nodegraph.rs

// The callback itself might return Update::DoNothing, but it invokes a user-provided callback
// which is expected to return the final Update status.
extern "C" fn nodegraph_input_output_connect(/*...*/) -> Update {
    // ...
    let result = match backref.callbacks.on_node_connected.as_mut() {
        Some(OnNodeConnected { callback, data }) => {
            // This user-provided callback will modify their application model
            // and return Update::RefreshDom to signal a redraw.
            let r = (callback.cb)(
                data,
                info,
                input_node,
                input_index,
                output_node,
                output_index,
            );
            backref.last_input_or_output_clicked = None;
            r // The final Update value comes from here.
        }
        None => Update::DoNothing,
    };

    result
}
```

This architecture solves the non-linear hierarchy problem. The communication path is a direct, 
logical link established by the backreferences, completely independent of whether the nodes are 
visual siblings, cousins, or in different panels (or even windows or threads, technically). 
This is the power of separating the **State Graph** from the **Visual Tree**.
