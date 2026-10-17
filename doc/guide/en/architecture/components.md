---
slug: architecture/components
title: Components
language: en
canonical_slug: architecture/components
audience: external
maturity: mature
guide_order: 42
topic_only: false
short_desc: Structuring reusable UI widgets and libraries
prerequisites: [dom, events/callbacks, architecture/routing]
default-search-keys:
  - ComponentLibrary
  - ComponentDef
  - ComponentDataModel
---

# Components

## Introduction

In Azul, a "component" is a reusable piece of UI - like a button, a "navigation bar", 
or a "user profile card". It encapsulates the structure and callbacks (`Dom`), the 
styling (`Css`), and the widget state (`RefAny`) required to render it.

The component system bridges the gap between code-driven immediate-mode GUIs and the
visual drag-and-drop `AzBuilder` application - see the next tutorial if you want to skip ahead.
This tutorial is mainly necessary if you want to create new components that the GUI builder
can then preview and use, as well as if you want to bundle your custom components in a library
that other people can install.

## Idea

The idea of components is that you can use the GUI builder to design the visual style,
let it generate all the tedious `Dom::create_foo` calls and then wrap it in a "component"
for other people to register in their applications. `AzBuilder` internally uses a JSON-ish
model to define the widget tree and gives you a live preview and introspection.

Components are good for:

1. Introspection: When you open `AzBuilder`, the browser requests the `ComponentMap` to list 
   and preview all components and their data model.
2. Dynamic UI: The browser reads the `ComponentDataModel` for the selected component. If 
   it sees a `ComponentFieldType::String` named "Title", it generates a text box. 
   If it sees an enum, it generates a dropdown.
3. Hot-Reloading: When you change a value in the browser, the browser simply patches 
   the `ComponentDataModel` and sends the updated JSON of the model back to the native app, 
   so that it can preview it. 
4. Instant Rendering: The native app uses the component's `render_fn` to update the native windows 
   preview without recompiling.

## Structure

A component is a self-describing bundle (a `ComponentDef`). Every component consists 
of three items:

The `ComponentDataModel` describes the serialized schema that defines exactly what properties 
(`props`) the component accepts. It defines field names, their types (strings, floats, enums, 
callbacks), and their default values. It's effectively the "metadata" of the `RefAny` that 
will be the public API of your component.

```rust
struct ComponentDataField {
    name: AzString, // "counter", "text", "number"
    field_type: ComponentFieldType, // "i32", "string", etc.
    default_value: Option<ComponentDefaultValue>,
    required: bool,
    description: String,
}

struct ComponentDataModel {
    name: String, // "UserProfile", "TodoItem"
    description: String,
    fields: Vec<ComponentDataField>,
}

ComponentDef {
    id: ComponentId, // "builtin:div", "shadcn:avatar"
    display_name: String, // "Link", "Avatar"
    description: String,
    source: ComponentSource, // builtin, compiled, user-defined
    data_model: ComponentDataModel,
    render_fn: ComponentRenderFn, // f(Data) -> Dom
    compile_fn: ComponentCompileFn, // f(Definition) -> SourceCode
    /// Source code for `render_fn` (user-defined components only)
    render_fn_source: Option<String>, // render_fn for runtime-defined components
    compile_fn_source: Option<String>, // compile_fn for runtime-defined components
}
```

## Composing Components

The `render_fn` is a pure callback function that takes the populated `ComponentDataModel` and 
returns a visual Dom. Since the `Css` is attached to the `Dom` object, this also 
configures all the styling, so that the visual description is self-contained:

```rust
// render_fn
fn(&ComponentDef, &ComponentDataModel, &ComponentMap) -> Result<Dom, RenderDomError>
```

The `render_fn` can use the `ComponentMap` to recursively instantiate its sub-components.

The `compile_fn` is the inverse of the render function: it takes a `ComponentDef` and a `CompileTarget` 
and returns the raw source code string (in Rust, Go, C, etc.) that would natively construct this 
component.

```rust
// compile_fn
fn(&ComponentDef, &ComponentDataModel, &CompileTarget) -> Result<String, CompileError>
```

Usually you create components visually with the drag-and-drop GUI builder, which at first 
simply calls `.with_child((sub_component.render_fn)())` and updates the visual preview.
Internally, however, it also creates a hierarchy of `ComponentDef`s - in line with the 
"backreference" architecture from the [Architecture Tutorial](../architecture.md).

The idea is that then any screen simply becomes a nested hierarchy of `ComponentDef` models
during code generation, i.e.:

```rust
struct GeneratedDataModel {
    card_data: CardDataModel {
        title: String,
        content: MarkdownDataModel {
            text: String,
        }
    },
    avatar_data: AvatarDataModel {
        picture: ImageRef,
        name: String,
    }
}
```

The `f(RefAny) -> Dom` then only has to "wire up" the respective fields to its data 
model, so that the internal UI structure of the components themselves is abstracted away.

The code generator can then generate the parent components `render_fn` and `compile_fn` - so 
that each `Component` only has to care about its direct children (fields) and recursion 
handles the rest:

```rust
impl AvatarDataModel {
    fn render(model: &Self) -> Dom {
        Dom::create_image(model.picture)
        .with_child(Dom::create_p_with_text(model.name))
    }

    fn generate_code() -> String {
        format!("
            fn render_avatar_data_model(model: AvatarDataModel) -> Dom {{
                Dom::create_image(model.picture)
                .with_child(Dom::create_p_with_text(model.name))
            }}
        ")
    }
}

impl GeneratedDataModel {

    fn render(model: &Self) -> Dom {
        CardDataModel::render(&model.card_data)
        .with_child(&AvatarModel::render(&model.avatar_data))
    }

    fn generate_code() -> String {
        let code_to_render_card = CardDataModel::generate_code();
        let code_to_render_avatar = AvatarModel::generate_code();

        format!("
            fn render_generated_data_model() -> Dom {{
              {code_to_render_card}
              .with_child({code_to_render_avatar})
            }}
        ")
    }
}
```

Note: These examples have been heavily simplified, the APIs are not exact, 
but they demonstrate how the recursion is supposed to work, both in visual 
preview rendering and in code generation. In reality these functions are a 
bit more complex, since they need to pass down parameters.

After you've exported the code, you can then "refine" the components public 
API and add callbacks (there is no on-the-fly recompilation yet, so `AzBuilder` 
can only do the visual part for now). However, the idea is to instantly get a 
preview and not having to recompile to test your component in various theme or 
OS environments.

## Built-in Components

The standard HTML elements (`<div>`, `<p>`, `<button>`) are provided out of the box 
as part of a pre-registered `builtin` component library. Whenever you parse XML (via 
`Dom::from_xhtml`) or use DOM builder methods, you are only instantiating these 
built-in components under the hood.

```xml
<div class="card">
    <h1>Hello World</h1>
    <p>Welcome to Azul</p>
</div>
```

is equal to:

```xml
<builtin:div class="card">
    <builtin:h1>Hello World</builtin:h1>
    <builtin:p>Welcome to Azul</builtin:p>
</builtin:div>
```

Or, in JSON form:

```json
[{
    "library": "builtin",
    "component": "div",
    "classes": ["card"],
    "children": [
        {
            "library": "builtin",
            "component": "h1",
            "text": "Hello World"
        },
        {
            "library": "builtin",
            "component": "p",
            "text": "Welcome to Azul"
        }
    ]
}]
```

This way, `AzBuilder` can import and export custom-built components and 
output the required source code for custom, runtime-defined components.

## Creating Custom Components

To create your own component in code, you need to define two main things:

1. Data Model: The properties (or "props") your component accepts.
2. Render Function: The logic that takes those properties and returns a `Dom`.

A `ComponentDataModel` is a list of named fields and their types 
(e.g., strings, booleans, or callbacks) along with their default values.

```rust,ignore
use azul::prelude::*;

fn my_card_model() -> ComponentDataModel {
    let mut model = ComponentDataModel::default();
    
    // Add a 'title' string property
    model.fields.push(ComponentDataField {
        name: AzString::from("title"),
        field_type: ComponentFieldType::String,
        default_value: Some(ComponentDefaultValue::String(AzString::from("Default Title"))).into(),
        required: false,
    });
    
    // Add a 'body' string property
    model.fields.push(ComponentDataField {
        name: AzString::from("body"),
        field_type: ComponentFieldType::String,
        default_value: Some(ComponentDefaultValue::String(AzString::from("Default body text"))).into(),
        required: false,
    });
    
    model
}
```

The render function receives the parsed properties from the data model and constructs the UI. 

```rust,ignore
fn render_my_card(
    _def: &ComponentDef,
    model: &ComponentDataModel,
    _map: &ComponentMap,
) -> Result<StyledDom, RenderDomError> {
    
    // Extract properties from the model
    let title = model.get_default_string("title")
        .cloned().unwrap_or(AzString::default());

    let body = model.get_default_string("body")
        .cloned().unwrap_or(AzString::default());
    
    let css = AzString::from(".my-card { 
        padding: 20px; 
        border: 1px solid #ccc; 
    }");

    // Build the DOM
    let dom = Dom::create_div()
        .with_class("my-card")
        .with_child(Dom::create_h2_with_text(title))
        .with_child(Dom::create_p_with_text(body));
        
    Ok(dom.style(css))
}
```

## Registering Component Libraries

Components are grouped into a `ComponentLibrary`, which is then registered with the 
application via the `AppConfig`. Registration uses a callback function so that it can 
be cleanly bridged across languages (C, Python, etc.).

```rust,ignore
extern "C" 
fn register_my_library() -> ComponentLibrary {
    let mut lib = ComponentLibrary {
        name: AzString::from("mylib"),
        version: AzString::from("1.0.0"),
        ..Default::default()
    };
    
    // Add our custom card component to the library
    lib.components.push(ComponentDef {
        name: AzString::from("card"),
        data_model: my_card_model(),
        render_fn: render_my_card,
        ..Default::default()
    });
    
    lib
}

fn main() {
    let mut config = AppConfig::create();
    
    // Register the library under the namespace "mylib"
    config.add_component_library(
        AzString::from("mylib"), 
        register_my_library
    );

    // ... App::create(data, config)
}
```

## Using Custom Components

Once registered, your components are available globally in the XML parser and in 
the `AzBuilder`. To differentiate them from built-in HTML tags, you prefix them 
with your library's namespace. For example, to use the `card` component from `mylib`:

```xml
<mylib:card title="User Profile" body="Welcome back, Alice!" />
```

When the framework parses this XML:

1. It resolves `<mylib:card>` to your registered `ComponentDef`.
2. It populates the `ComponentDataModel` using the XML attributes (`title` and `body`).
3. It calls your `render_fn` to generate the final UI.

Once you're happy with the visual layout in the editor, the `compile_fn` 
(the inverse of `render_fn`) takes the customized data model and generates the raw source code 
(e.g., `fn card(...)`) to paste back into your project across any supported language 
(Rust, C, Go, etc.).
