# Azul Against The World(s Frameworks): Comparison

## Is Azul just rebranding old concepts?

### Azul vs. Qt (Signals & Slots)

```cpp
class MyWidget : public QWidget {
    QLineEdit* input;
    QLabel* output;
    
    MyWidget() {
        connect(input, &QLineEdit::textChanged, 
                this, &MyWidget::onInputChanged);
    }
    
    void onInputChanged(const QString& text) {
        output->setText(process(text));
    }
};
```

So, is Azul just signals & slots? No, but there's overlap:

| Concept | Qt | Azul |
|---------|----|----- |
| Connection mechanism | `connect(sender, signal, receiver, slot)` | `Dom::callback(fn, RefAny)` |
| State location | In widget objects | In `RefAny` (decoupled) |
| Visual tree coupling | **Tightly coupled** - slots are methods on QWidget subclasses | **Decoupled** - callbacks take `RefAny`, not UI objects |

Qt still fuses the Visual Tree and State Graph through inheritance. Azul separates them:

```cpp
// Qt: Logic IS-A widget
class MyLogic : public QWidget { /* ... */ }

// Azul: Lgic HAS-A reference
struct MyLogic { state: RefAny }
```

### Azul vs. Elm Architecture

```elm
type Msg = Increment | Decrement

update : Msg -> Model -> Model
update msg model =
    case msg of
        Increment -> { model | count = model.count + 1 }
        Decrement -> { model | count = model.count - 1 }

view : Model -> Html Msg
view model = 
    div [] [ button [ onClick Increment ] [ text "+" ] ]
```

Is Azul just Elm with mutation? Partially yes:

| Concept | Elm | Azul |
|---------|-----|------|
| `UI = f(data)` | ✓ Pure function | ✓ `layout(RefAny) -> Dom` |
| Central update | ✓ Single `update : Msg -> Model -> Model` | ✗ Callbacks mutate directly |
| Message passing | ✓ Everything goes through `Msg` | ✗ Callbacks have direct backreferences |

Azul keeps `UI = f(data)` but throws out the central message dispatcher. 
This is a genuine architectural difference, not just syntax:

```elm
-- Elm: EVERYTHING routes through update
update msg model =
    case msg of
        UserClickedNodePort portId -> 
            case msg of NodePortClick nodeId portId -> ...
        NodeDragStarted nodeId -> ...
        NodeDragMoved delta -> ...
        -- 500 more cases for complex apps

-- Azul: Logic is co-located
extern "C" fn on_port_click(data: RefAny, info: CallbackInfo) -> Update {
    let graph = data.downcast_mut::<NodeGraph>().unwrap();
    graph.start_connection(info.port_id); // direct access
    Update::RefreshDom
}
```

### Azul vs. React Hooks

```jsx
function Counter() {
    const [count, setCount] = useState(0);
    const [items, setItems] = useState([]);
    
    useEffect(() => {
        fetch('/items').then(setItems);
    }, []);
    
    return <button onClick={() => setCount(count + 1)}>{count}</button>;
}
```

Is Azul just React without JSX? No:

| Concept | React | Azul |
|---------|-------|------|
| State storage | Component-local (`useState`) | External (`RefAny`) |
| Re-render trigger | Framework decides via reconciliation | You decide via `Update::RefreshDom` |
| Side effects | `useEffect` with dependency tracking | `Task` with explicit spawning |
| Data flow | Down the tree (unless Context/Redux) | Arbitrary graph via backreferences |

React's constraint (framework-controlled renders) is fundamental. Azul inverts the control:

```jsx
// React: Framework owns the lifecycle
function MyComponent() {
    const [x, setX] = useState(0);
    // React calls this function on every relevant state change
    // You don't control WHEN
}

// Azul: You own the lifecycle
extern "C" fn layout(data: RefAny, _: LayoutCallbackInfo) -> StyledDom {
    // Only called when YOU return Update::RefreshDom
}

extern "C" fn on_click(data: RefAny, _: CallbackInfo) -> Update {
    data.downcast_mut::<State>().unwrap().x += 1;
    // You explicitly choose to refresh
    Update::RefreshDom 
}
```

### Azul vs. SwiftUI / Jetpack Compose

```swift
struct ContentView: View {
    @State private var count = 0
    @EnvironmentObject var settings: Settings
    
    var body: some View {
        Button("Count: \(count)") {
            count += 1
        }
    }
}
```

Is Azul just SwiftUI for Rust? **Surprisingly close**, but no:

| Concept | SwiftUI | Azul |
|---------|---------|------|
| Declarative UI | ✓ `body` property | ✓ `layout()` function |
| State binding | `@State`, `@Binding` down tree | `RefAny` anywhere in graph |
| Environment injection | `@EnvironmentObject` | Backreference or tunneling |
| Compile-time optimization | ✓ View tree diffing | ✓ **Const CSS compilation** |

SwiftUI got close with property wrappers, but still constrains state to the 
view tree. Azul's **explicit State Graph** is the key difference:

```swift
// SwiftUI: State is STILL tree-constrained
struct ParentView: View {
    @State var sharedData: Data
    
    var body: some View {
        VStack {
            ChildA(data: $sharedData) // binding flows down
            ChildB(data: $sharedData) // must wire through parent
        }
    }
}

// Azul: State graph is independent
let shared = RefAny::new(SharedData::default());
let child_a = ChildA { data: shared.clone() };
let child_b = ChildB { data: shared.clone() };
// No parent needed to wire them together
```

### Azul vs. Dear ImGui

```cpp
void RenderUI() {
    static int counter = 0;
    ImGui::Text("Counter: %d", counter);
    if (ImGui::Button("Increment")) {
        counter++;
    }
}
```

Is Azul just retained-mode ImGui? No:

| Concept | ImGui | Azul |
|---------|-------|------|
| Mode | Immediate (no persistent UI objects) | Retained (DOM persists) |
| State location | User's stack/globals | `RefAny` managed by framework |
| Separation of concerns | None - logic mixed with rendering | Clean - callbacks separate from layout |

ImGui's immediate mode is fundamentally different. Azul shares basically nothing:

```cpp
// ImGui: Rendering IS state management
void Render() {
    static int x = 0; // where does this live? global? stack?
    if (Button("Click")) { x++; } // logic IN render loop
}

// Azul: Rendering and logic are separate
extern "C" fn layout(data: RefAny, _: LayoutCallbackInfo) -> StyledDom {
    let x = data.downcast_ref::<State>().unwrap().x;
    Dom::button(&format!("x = {}", x))
        .with_callback(increment, data.clone())
}

extern "C" fn increment(data: RefAny, _: CallbackInfo) -> Update {
    data.downcast_mut::<State>().unwrap().x += 1;
    Update::RefreshDom
}
```

### Azul vs. Flutter

```dart
class Counter extends StatefulWidget {
    @override
    _CounterState createState() => _CounterState();
}

class _CounterState extends State<Counter> {
    int count = 0;
    
    @override
    Widget build(BuildContext context) {
        return ElevatedButton(
            onPressed: () => setState(() { count++; }),
            child: Text('$count'),
        );
    }
}
```

Is Azul just Flutter for native? Similar philosophy, different execution:

| Concept | Flutter | Azul |
|---------|---------|------|
| Declarative | ✓ `build()` | ✓ `layout()` |
| Retained widget tree | ✓ | ✓ |
| State management | `StatefulWidget` + `InheritedWidget` | `RefAny` + backreferences |
| Rendering | Custom engine (Skia) | Native (or GPU) |

Flutter is the closest modern equivalent. The key difference is Azul's **formal State Graph independence**, 
but the overall model is similar.

```dart
// Flutter: State still owned by widget
class MyWidget extends StatefulWidget {
    final Data data; // must pass through constructor
    
    @override
    Widget build(BuildContext context) {
        // Can only access what was passed down
    }
}

// Azul: State graph is separate
struct MyWidget {
    any_data_i_want: RefAny, // direct reference to any part of app
}

impl MyWidget {
    fn dom(&self) -> Dom {
        // Can access anything via backreferences
    }
}
```

### Azul vs. Yew (Rust React)

```rust
#[function_component]
fn Counter() -> Html {
    let counter = use_state(|| 0);
    let onclick = {
        let counter = counter.clone();
        Callback::from(move |_| counter.set(*counter + 1))
    };
    
    html! {
        <button {onclick}>{ *counter }</button>
    }
}
```

Is Azul just Yew without macros? No:

| Concept | Yew | Azul |
|---------|-----|------|
| Paradigm | React hooks in Rust | Custom architecture |
| Component state | `use_state` (component-local) | `RefAny` (external) |
| Re-render control | Framework decides | Explicit `Update::RefreshDom` |
| Target | WASM/Web | Native/Desktop |

Yew is "React in Rust." Azul is architecturally different.

```rust
// Yew: Still React's model
#[function_component]
fn App() -> Html {
    let state = use_state(|| AppState::default());
    // Framework calls this function to re-render
}

// Azul: Different control flow
extern "C" fn layout(data: RefAny, _: LayoutCallbackInfo) -> StyledDom {
    // Only called when you explicitly request it
}
```

### Azul vs. Vanilla JS

```javascript
const button = document.createElement('button');
button.textContent = 'Click me';
button.addEventListener('click', () => {
    appState.counter++;
    updateUI(); // manual sync
});
document.body.appendChild(button);
```

Is Azul just a fancy DOM API? No:

| Concept | Vanilla JS | Azul |
|---------|-------------|------|
| Declarative | ✗ Imperative | ✓ Declarative |
| Sync | Manual | Automatic (on `Update::RefreshDom`) |
| State management | DIY | `RefAny` with downcasting |

## The Novel Contributions

So, after comparing against everything, here's what Azul *actually* contributes:

### Formalized State Graph

```rust
// Explicitly build the logical graph
struct NodeGraph {
    nodes: Vec<NodeWidget>,
    connections: Vec<Connection>,
}

struct NodeWidget {
    graph_ref: RefAny, // explicit edge in State Graph
}

struct PortWidget {
    node_ref: RefAny, // explicit edge in State Graph
}
```

**No other toolkit makes this explicit:**

- Qt: Signals & slots *approximate* this but are still tied to QObject hierarchy
- React: Requires Context/Redux workarounds
- SwiftUI: `@EnvironmentObject` is close but still tree-constrained
- Flutter: `InheritedWidget` has same tree constraint

Backreferences as a *primary architectural pattern* for building an arbitrary 
State Graph independent of the Visual Tree.

### Explicit Update Control

```rust
extern "C" fn callback(data: RefAny, _: CallbackInfo) -> Update {
    // Programmer decides: refresh or not?
    if data.downcast_ref::<State>().unwrap().needs_render {
        return Update::RefreshDom;
    }
    Update::DoNothing
}
```
The user has explicit control over the render cycle without sacrificing declarative UI.

- React: Framework decides when to re-render
- Elm: `update` always produces new model, framework handles rendering
- ImGui: Renders every frame
- SwiftUI: Framework decides based on `@State` changes

###  Compile-Time CSS Compilation

```rust
const CSS_MATCH_17553577885456905601: NodeDataInlineCssPropertyVec = /* ... */;
const LIST_VIEW_NEVER_CHANGES: StyledDom = StyledDom::div()
    .with_inline_css_props(CSS_MATCH_17553577885456905601);
```

No other toolkit does this. The closest is:

- Qt: can compile QuickJS to C++ with **proprietary** compiler
- Flutter: Partially optimizes widget trees at compile time
- SwiftUI: View diffing is optimized but not CSS-specific

### RefAny with Module-Scoped Downcasting

```rust
// Public API
pub struct NumberInput {
    internal: RefAny, // type hidden
}

// Private implementation
struct NumberInputInternal { /* ... */ }

extern "C" fn callback(data: RefAny, _: CallbackInfo) -> Update {
    // Only this module can downcast
    let d = data.downcast_mut::<NumberInputInternal>().unwrap();
}
```

Similar concepts exist, but the combination is new.

- Qt: `QVariant` (but runtime type checking is less safe)
- Web: `Object` (completely untyped)
- Rust: `Box<dyn Any>` (but no module-scoped privacy pattern)

The combination itself is new: Type-safe polymorphism + module-enforced 
encapsulation + reference-counted lifecycle.

## Final Verdict

Azul is about 60 - 70% genuinely new. The main new parts are:

1. **Formalized State Graph as first-class architecture** (vs escape hatches like Redux/Context)
2. **RefAny pattern for polymorphic state** (novel combination)
3. **Explicit update control** (you decide when to re-render)
4. **Compile-time CSS compilation** (unique optimization)

The borrowed parts:

1. Declarative `UI = f(data)` (from React/Elm/Flutter)
2. Retained mode DOM (from web/React/Flutter)
3. Callbacks/event handlers (universal)
4. Component composition (universal)

Can existing toolkits easily adopt Azul's model?

- **React:** No - would require abandoning the reconciler and component lifecycle
- **Qt:** No - would require abandoning QObject inheritance
- **SwiftUI:** Maybe - property wrappers could theoretically support it, but would be a major redesign
- **Flutter:** Maybe - InheritedWidget could be extended, but would break existing patterns

If they can't adopt it without fundamental rewrites, it's genuinely new.
