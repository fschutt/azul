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

## Second Gen: Constrained Hierarchy (Elm, React)

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
panel? 

React's solution is to "lift state up" to their lowest common ancestor, `MyApp`. The `MyApp` 
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

Elms solution is to "lift the state up" to the root ancestor and route everything in a single "update" 
function. Elm therefore represents the philosophical extreme of the constrained hierarchy:

1.  **Model:** The entire state of the application is held in a single, immutable data structure.
2.  **View:** A pure function that takes the `Model` and returns a description of the UI.
3.  **Update:** A single, central function that is the *only* entity allowed to modify the state.
    It does so by taking an incoming `Msg` (a message from the UI) and the current state, and
    producing a *new* state.

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

## Quid est GUI, Claude-ia?

So, what *is* a "GUI toolkit"? How does it differ from just a rendering library?

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

The first thing we'd need to decide is whether we'd like to serialize the UI or render it directly, 
without storing it. Now, as computers got faster and rendering methods evolved, UI toolkits moved 
away from a direct render-to-screen to a display-list or a "list of commands" for the renderer. 
Often times this command list is batched or computed against the last frame to minimize changes - 
while there is a small overhead, it is almost unnoticeable.

The only real "sane" way here is to serialize the entire UI hierarchy and then perform layout, 
state and cache analysis in the toolkit. A good comparison is to compare XML to function call 
stacks - compare:

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

Composing UI hierarchies via functions makes much more sense than composing UI hierarchies via 
inheritance because the latter is often language-specific and not supported in all languages, 
whereas functions are language agnostic.

## Data access: Format and locality

The second decision is where to store the UI data, so that the callbacks may access it again.
Widget-specific data has to be either stored on the programmer side (in the application, using 
inheritance or traits) or in the framework (either using data attributes or - worse - global state 
modifying functions such as synchronous `setColor(RED); draw(); swap();` calls). What format should we use?

Inheritance-based toolkits only allow one format: You have to inherit from a UI object and then construct 
your application as a series of UI objects. Azul however, stores the application data as an 
implementation-agnostic `RefAny` struct: similar to `PyObject` or Javascripts `Object` it just stores 
"some data", but the toolkit doesn't know what the type is. You can upcast your data and wrap it via 
`RefAny::new` and then get immutable or mutable access again via `.downcast_ref()` or `.downcast_mut()`, 
respectively:

```rust
let data = RefAny::new(5); // owns the data
let data_clone = data.clone(); // only bumps the reference count

let data_ref: &usize = data.downcast_ref::<usize>().unwrap(); // ok
println!("{}", *data); // prints "5"

let data_mut: &mut usize = data.downcast_ref::<usize>().unwrap(); // error: data_ref still held
// object destroyed here
```

Note: The downcast of a `RefAny` is also thread-safe and type-checked (even in C or Python). 
Effectively this is similar to `Observables`, however, since `RefAny`s are connected to a `Callback`, 
a `Dom`, a `Task` or a `Thread`, the **topology** of how they are connected is more obvious than 
with a free-floating `Observable`, whose memory lives "somewhere".

Using [insert language]s module system, we can control any errors related to up / downcasting:

```rust
// number_input.rs (private internals)
struct NumberInputInternal { /* ... */ }

// number_input.rs (public API)
pub struct NumberInput {
    internal: RefAny, // holds NumberInputInternal
}

impl NumberInput {
    pub fn dom(self) -> Dom {
        Dom::new().with_callback(private_callback, self.internal)
    }
}

extern "C"
fn private_callback(data: RefAny, info: CallbackInfo) -> Update {
    // only code in this module can downcast to NumberInputInternal
    // external code can't even name the type, so no downcast error possible
    let d = data.downcast::<NumberInputInternal>().unwrap();
}
```

When all references to a `RefAny` are deleted, the internal object is deleted, too.

## Building a State Graph

Azul acknowledges the declarative revolution of `UI = f(data)` but also recognizes the 
architectural limitations of the constrained hierarchy. It provides a clean, formal 
architecture for building the State Graph directly, independent of the Visual Tree. 
To see these principles in action, let's examine a notoriously difficult UI component: 
a `NodeGraph`.

The challenge of a node graph is that the *logical* connections between nodes (a complex graph) 
have no relation to their *visual* layout (a flat list of sibling elements on a canvas). 
This breaks the core assumptions of almost every other toolkit. Azul solves this through two 
primary communication mechanisms: **backreferences** and **tunneling**.

### Backreferences: The Clean Path

The primary and most powerful pattern in Azul is the **backreference**. This is 
a reference (`RefAny`) to a higher-level data model that is passed down to a 
lower-level component when it's created. This creates a direct, explicit edge 
in the State Graph.

In the NodeGraph, when a user clicks an input port on a node, how does it tell the 
top-level `NodeGraph` state to create a connection? It doesn't send a message "up" 
the Visual Tree. Instead, it follows a pre-defined chain of backreferences:

1.  The `Dom` for the input port has a callback holding a `PortWidget`'s data.
2.  This `PortWidget` contains a backreference to its logical parent's data, the `NodeWidget`.
3.  The `NodeWidget` in turn holds a backreference to the top-level `NodeGraphWidget`, which contains
    the entire application state.

The callback for the click event on a visual node's `Input` / `Output` simply follows 
this chain of references, making a direct jump from the event source to the top-level data model.

```python
# Python pseudo-code illustrating the backreference chain
# NOTE: These are not UI elements, but the logical controllers for them.

class NodeGraphWidget:
    def __init__(self, graph_state):
        self.graph_state = graph_state  # The actual application data

    # Logic that lives at the top level
    def on_port_clicked(self, port_id):
        print(f"LOGIC(NodeGraph): Port {port_id} clicked. Updating global state.")
        # ... logic to connect nodes in self.graph_state ...

class NodeWidget:
    def __init__(self, node_id, graph_widget_ref):
        self.node_id = node_id
        self.graph_widget_ref = graph_widget_ref  # Backreference to the graph

    # This method is "lent" to the PortWidget
    def on_port_clicked(self, port_id):
        print(f"LOGIC(Node): Click received for port {port_id}. Forwarding to graph.")
        # Uses its backreference to call the top-level logic
        self.graph_widget_ref.on_port_clicked(port_id)

class PortWidget:
    def __init__(self, port_id, node_widget_ref):
        self.port_id = port_id
        self.node_widget_ref = node_widget_ref  # Backreference to the node

    # This would be the callback attached to the UI element
    def handle_click_event(self):
        print(f"EVENT on Port {self.port_id}")
        # Uses its backreference to start the logical chain
        self.node_widget_ref.on_port_clicked(self.port_id)
```

Wiring it all up:

```python
# Top-level state and logic controller
app_state = {"nodes": {}, "connections": []}
graph_controller = NodeGraphWidget(app_state)

# 2. Create controllers for child components, passing down backreferences
node_a_controller = NodeWidget("NodeA", graph_controller)
port_a1_controller = PortWidget("PortA1", node_a_controller)

# 3. Simulate a user clicking the visual port
port_a1_controller.handle_click_event()
```

The flow of control follows the logical graph, not a visual one:

1. `Event` -> `PortWidget.handle_click_event()` 
2. `PortWidget.handle_click_event()` -> `NodeWidget.on_port_clicked()` 
3. `NodeWidget.on_port_clicked()` -> `NodeGraphWidget.on_port_clicked()`

This data flow is *completely independent* of the visual layout. The `PortWidget` is 
perfectly decoupled; it doesn't know what the `NodeGraphWidget` is, only that it must 
call a function on the reference it was given.

#### Tunneling: The Visual Query

The second, more imperative way to access data is **tunneling**. Azul allows you to attach 
data to any DOM node via a `dataset`. From a callback, you can then "tunnel" to that node 
and retrieve its data if you know its `NodeId`.

While powerful, this pattern is less clean because it re-introduces a coupling between your logic 
and the Visual Tree. If you refactor your UI, it can't be statically assured that your callbacks 
won't break. You can however do things such as `callback_info.find_parent_nodeid(".my_class")` to
make the "NodeId" lookup more resilient:

```
extern "C"
fn my_callback(data: RefAny, cb: CallbackInfo) -> Update {
     let visual_parent: NodeId = cb.get_parent_id(cb.hit_node, ".my_class").unwrap();
     let dataset: RefAny = cb.get_dataset(visual_parent).unwrap();
     let mut downcasted = dataset.downcast_mut::<Foo>();
     downcasted.bar += 5.0;
     Update::DoNothing
}
```

Its proper use is for managing purely UI-related state that is not part of the core application 
model. In the `NodeGraph`, when a node is dragged, the callback needs to update the CSS `transform` 
property of the specific visual `div` for that node. The core `NodeGraph` data model shouldn't be 
polluted with toolkit-specific `NodeId`s. Instead, the drag callback can use tunneling to find the 
`NodeId` of the visual element it needs to manipulate, keeping the UI-specific logic separate from 
the application state.

Key takeaway: **backreferences build the State Graph, while tunneling queries the Visual Tree.**

## FAQ from [...] developers

### Isn't this just a more complex way of signals-and-slots or observer patterns?

No. While both are used for communication, signals-and-slots still require manual wiring 
between UI objects, often leading to a complex web of connections. Azul's backreferences 
let you create a formal *State Graph* that is independent of the UI layout, making data 
flow clearer and preventing your application logic from being tied to your visual design.

> "But my class hierarchy *is* my application structure. Separating them sounds like boilerplate."

Fusing your logic to the visual hierarchy makes refactoring the UI difficult and 
your code untestable outside the toolkit. Decoupling them allows your core logic to be 
independent, portable, and easier to test.

### How is this different from Redux / Context API?

Redux and Context are workarounds to the default tree-based data flow. Azul's **backreferences** 
are a primary, built-in architectural pattern, not an escape hatch. They allow you to directly 
and explicitly define your application's **State Graph** from the ground up, rather than having 
to route everything through a central store or a common ancestor.

> "But manually passing references down sounds like a return to prop drilling."

This isn't manual pointer management; it's defining the logical connections of your app. 
This explicitness makes complex interactions (like a node graph) far easier to reason about than 
tracking where context is provided or how actions are dispatched and mapped to state.

### Why isn't it using Elms `update()` model?

The central `update` function and message (`Msg`) types quicly grow enormous in large applications. 
Azul allows you to maintain a central data model (`UI = f(data)`) but provides a more direct and 
decentralized way for events to trigger logic via backreferences. It avoids routing every single 
interaction through one monolithic function.

> "A single `update` function is a feature, not a bug. It makes all state changes predictable and easy to debug."

Azul retains predictability, but the data flow is still unidirectional (**Event -> State Change -> Re-render**), 
but the "update" logic is co-located with the relevant part of the State Graph, making the system more modular 
and scalable without sacrificing clarity.

### SwiftUI and Compose already have `@State`, `@Binding`, and `@EnvironmentObject` to manage state declaratively.

Those tools are still fundamentally designed around the **Visual Tree**. `@EnvironmentObject` is similar 
to React's Context, and `@Binding` is a form of two-way data binding down the hierarchy. Azul's approach 
is to formally separate the **State Graph** from the view hierarchy completely (with the exception of 
tunneling, but this is already marked as "unclean"), which provides a cleaner solution for non-hierarchical 
data dependencies that often require complex workarounds in other frameworks.

### What about performance?

This is where Azul tricks a bit. **Due to its pure-functional nature**, the `Dom` can in fact be pre-computed to 
a `const` item, i.e. **constructed at compile time**. Using a separate tool, Azul can compile 
**HTML/CSS directly to Rust/C code**. It looks a bit like this:

```html
<style>
.__azul_native_list-container {
    
}
</style>
<div class="__azul_native_list-container"></div>
```

becomes (after compilation):

```rust
const CSS_MATCH_17553577885456905601_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native_list-container
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_2444935983575427872_ITEMS,
        )),
    )),
];

const CSS_MATCH_17553577885456905601: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_17553577885456905601_PROPERTIES);

const IDS_AND_CLASSES_9205819539370539587: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul_native_list-container",
))];

const LIST_VIEW_CONTAINER_CLASS: IdOrClassVec =
    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_9205819539370539587);

const LIST_VIEW_NEVER_CHANGES: StyledDom = StyledDom::div()
    .with_inline_css_props(CSS_MATCH_17553577885456905601)
    .with_ids_and_classes(LIST_VIEW_CONTAINER_CLASS);

extern "C"
fn layout(refany: RefAny, info: LayoutCallbackInfo) -> StyledDom {
    // doesn't actually clone anything, because it's all &'static
    return LIST_VIEW_NEVER_CHANGES.clone();
}
```

This avoids doing the "CSS cascade" at runtime and instead pushes it to compile time.
The `AzString` and `FooVec` types all allow you to create strings / arrays from compile-time
data, so the final "re-invocation" is a no-op for never-changing UI components and doesn't
require memory allocation.

Second, the Windows main `layout` callback is only re-invoked when the callback returns `Update::RefreshDom`,
and things like GPU transforms, animations or style modifications can be done without requiring calling `layout()` again.

Third, Azul has ways to manage infinite / sparse datasets and you only need to return in the DOM what is 
actually on-screen, which will be a few hundred DOM nodes at most. So, for the newcomer, Azul is easy to 
use at first with a simple programming model, while still allowing to optimize the performance heavily - 
once that actually is a problem. 

Fourth, Azul usese caches internally for everything, including the incremental HTML layout, so window 
resizing is incredibly fast.
