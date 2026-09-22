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

In Azul, a "component" is a reusable piece of UI - like a button, a navigation bar, 
or a user profile card. It encapsulates the structure and callbacks (`Dom`), the 
styling (`Css`), and the widget state (`RefAny`) required to render it.

While you can always break your UI down into regular functions that return `Dom` nodes, 
packaging them formally as a "Component Library" gives you superpowers: it allows your 
custom widgets to be instantiated from XML, inspected in a visual editor, live-previewed
without recompiling as well as later on compiled to a target language, such as Rust.

## Using Built-in Components

When instantiating (X)HTML
The standard HTML elements (`<div>`, `<p>`, `<button>`) are provided out of the box 
as part of a pre-registered `builtin` component library. 

Whenever you parse XML or use DOM builder methods, you are instantiating these built-in 
components under the hood.

```xml
<!-- Uses builtin:div, builtin:h1, and builtin:p -->
<div class="card">
    <h1>Hello World</h1>
    <p>Welcome to the application.</p>
</div>
```

## Creating Custom Components

To create your own component, you need to define two main things:

1. **The Data Model:** The properties (or "props") your component accepts.
2. **The Render Function:** The logic that takes those properties and returns a `StyledDom`.

### 1. Define the Data Model

A `ComponentDataModel` is a list of named fields and their types (e.g., 
strings, booleans, or callbacks) along with their default values.

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

### 2. Define the Render Function

The render function receives the parsed properties from the data model and constructs the UI. 

```rust,ignore
extern "C" fn render_my_card(
    _def: &ComponentDef,
    model: &ComponentDataModel,
    _map: &ComponentMap,
) -> ResultStyledDomRenderDomError {
    
    // Extract properties from the model
    let title = model.get_default_string("title").cloned().unwrap_or(AzString::from(""));
    let body = model.get_default_string("body").cloned().unwrap_or(AzString::from(""));
    
    // Build the DOM
    let dom = Dom::create_div()
        .with_class("my-card")
        .with_child(Dom::create_h2_with_text(title))
        .with_child(Dom::create_p_with_text(body));
        
    Ok(StyledDom::from_dom(dom))
}
```

## Registering Component Libraries

Components are grouped into a `ComponentLibrary`, which is then registered with the 
application via the `AppConfig`. Registration uses a callback function so that it can 
be cleanly bridged across languages (C, Python, etc.).

```rust,ignore
extern "C" fn register_my_library() -> ComponentLibrary {
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
        // (Optional) CSS styles specific to this component
        css: AzString::from(" .my-card { padding: 20px; border: 1px solid #ccc; } "),
        ..Default::default()
    });
    
    lib
}

fn main() {
    let mut config = AppConfig::create();
    
    // Register the library under the namespace "mylib"
    config.add_component_library(AzString::from("mylib"), register_my_library);

    // ... App::create(data, config)
}
```

## Using Custom Components in XML

Once registered, your components are available globally in the XML parser. 
To differentiate them from built-in HTML tags, you prefix them with your 
library's namespace.

For example, to use the `card` component from `mylib`:

```xml
<mylib:card title="User Profile" body="Welcome back, Alice!" />
```

When the framework parses this XML:

1. It resolves `<mylib:card>` to your registered `ComponentDef`.
2. It populates the `ComponentDataModel` using the XML attributes (`title` and `body`).
3. It calls your `render_fn` to generate the final UI.

## Why the explicit Data Model? (Live Preview & Codegen)

You might wonder why you have to build a `ComponentDataModel` instead of just writing a 
standard Rust function `fn my_card(title: String) -> Dom`.

The explicit data model is the secret sauce that powers Azul's visual tooling:

* **Live Preview:** The design-time tool reads your data model, generates a property-editor 
  UI (text boxes, color pickers, etc.), and allows you to tweak values. When a value changes, 
  it updates the model and instantly calls `render_fn` to show you the result—without recompiling 
  your app.
* **Code Generation:** When you're happy with the visual layout in the editor, the `compile_fn` 
  (the inverse of `render_fn`) takes the customized data model and generates the raw source code 
  (e.g., `fn card(...)`) to paste back into your project across any supported language (Rust, C, Go, etc.).

